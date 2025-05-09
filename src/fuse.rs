use byteorder::ByteOrder;

macro_rules! get_node {
    ($ef:expr, $nid:expr) => {
        $ef.get_node($nid).unwrap()
    };
}

macro_rules! get_node_mut {
    ($ef:expr, $nid:expr) => {
        $ef.get_node_mut($nid).unwrap()
    };
}

macro_rules! debug_req {
    ($req:expr, $cond:expr) => {
        if $cond {
            log::debug!("{:?}", $req);
        }
    };
}

// libexfat (both relan/exfat and Rust) isn't thread-safe.
// relan/exfat uses libfuse in single-thread mode (-s option).
static MTX: std::sync::LazyLock<std::sync::Mutex<i32>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(0));

macro_rules! mtx_lock {
    ($mtx:expr) => {
        $mtx.lock().unwrap()
    };
}

const TTL: std::time::Duration = std::time::Duration::from_secs(1);

fn stat2attr(st: &libexfat::exfat::Stat) -> fuser::FileAttr {
    let attr = crate::util::stat2attr(st);
    log::debug!("{attr:?}");
    attr
}

#[allow(clippy::needless_pass_by_value)]
fn e2i(e: libexfat::Error) -> i32 {
    (match e {
        libexfat::Error::Errno(e) => e,
        libexfat::Error::Error(e) => match libfs::os::error2errno(&e) {
            Some(v) => v,
            None => nix::errno::Errno::EINVAL,
        },
    }) as i32
}

impl fuser::Filesystem for crate::ExfatFuse {
    fn init(
        &mut self,
        req: &fuser::Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        debug_req!(req, self.debug > 1);
        log::debug!("config {config:?}");
        let _mtx = mtx_lock!(MTX);
        // mark super block as dirty; failure isn't a big deal
        if let Err(e) = self.ef.soil_super_block() {
            return Err(e2i(e));
        }
        Ok(())
    }

    fn destroy(&mut self) {
        log::debug!("destroy");
        let _mtx = mtx_lock!(MTX);
        assert_eq!(self.total_open, 0);
        self.ef.unmount().unwrap();
    }

    fn lookup(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("dnid {dnid} name {name:?}");
        let _mtx = mtx_lock!(MTX);
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.lookup_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                get_node_mut!(self.ef, nid).put();
                reply.error(e2i(e));
                return;
            }
        };
        get_node_mut!(self.ef, nid).put();
        reply.entry(&TTL, &stat2attr(&st), 0);
    }

    fn getattr(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: Option<u64>,
        reply: fuser::ReplyAttr,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid}");
        let _mtx = mtx_lock!(MTX);
        if let Some(fh) = fh {
            assert_eq!(nid, fh);
        }
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        reply.attr(&TTL, &stat2attr(&st));
    }

    #[allow(clippy::similar_names)]
    #[allow(clippy::too_many_lines)]
    fn setattr(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        ctime: Option<std::time::SystemTime>,
        fh: Option<u64>,
        crtime: Option<std::time::SystemTime>,
        chgtime: Option<std::time::SystemTime>,
        bkuptime: Option<std::time::SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        debug_req!(req, self.debug > 1);
        if self.debug > 0 {
            let mut s = format!("nid {nid}");
            if let Some(fh) = fh {
                s = format!("{s} fh {fh}");
            }
            if let Some(mode) = mode {
                s = format!("{s} mode {mode:#o}");
            }
            if let Some(uid) = uid {
                s = format!("{s} uid {uid}");
            }
            if let Some(gid) = gid {
                s = format!("{s} gid {gid}");
            }
            if let Some(size) = size {
                s = format!("{s} size {size}");
            }
            if let Some(atime) = atime {
                s = format!("{s} atime {atime:?}");
            }
            if let Some(mtime) = mtime {
                s = format!("{s} mtime {mtime:?}");
            }
            if let Some(ctime) = ctime {
                s = format!("{s} ctime {ctime:?}");
            }
            if let Some(crtime) = crtime {
                s = format!("{s} crtime {crtime:?}");
            }
            if let Some(chgtime) = chgtime {
                s = format!("{s} chgtime {chgtime:?}");
            }
            if let Some(bkuptime) = bkuptime {
                s = format!("{s} bkuptime {bkuptime:?}");
            }
            if let Some(flags) = flags {
                s = format!("{s} flags {flags:#x}");
            }
            log::debug!("{s}");
        } else {
            log::debug!("nid {nid}");
        }
        let _mtx = mtx_lock!(MTX);
        if let Some(fh) = fh {
            assert_eq!(nid, fh);
        }
        let mut st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        if let Some(mode) = mode {
            let mode_mask =
                libc::S_IFREG | libc::S_IFDIR | libc::S_IRWXU | libc::S_IRWXG | libc::S_IRWXO;
            #[cfg(target_os = "linux")]
            let valid_mode_mask = mode_mask;
            #[cfg(not(target_os = "linux"))] // FreeBSD
            let valid_mode_mask = u32::from(mode_mask);
            if (mode & !valid_mode_mask) != 0 {
                reply.error(libc::EPERM);
                return;
            }
        }
        if let Some(uid) = uid {
            if uid != st.st_uid {
                reply.error(libc::EPERM);
                return;
            }
        }
        if let Some(gid) = gid {
            if gid != st.st_gid {
                reply.error(libc::EPERM);
                return;
            }
        }
        if let Some(size) = size {
            get_node_mut!(self.ef, nid).get();
            if let Err(e) = self.ef.truncate(nid, size, true) {
                if self.ef.flush_node(nid).is_err() {
                    // ignore this error
                }
                get_node_mut!(self.ef, nid).put();
                reply.error(e2i(e));
                return;
            }
            if let Err(e) = self.ef.flush_node(nid) {
                get_node_mut!(self.ef, nid).put();
                reply.error(e2i(e));
                return;
            }
            // truncate has updated mtime
            st = match self.ef.stat(nid) {
                Ok(v) => v,
                Err(e) => {
                    get_node_mut!(self.ef, nid).put();
                    reply.error(e2i(e));
                    return;
                }
            };
            get_node_mut!(self.ef, nid).put();
            st.st_size = size;
        }
        let mut attr = crate::util::stat2attr(&st);
        if let Some(atime) = atime {
            attr.atime = match atime {
                fuser::TimeOrNow::SpecificTime(v) => v,
                fuser::TimeOrNow::Now => std::time::SystemTime::now(),
            };
        }
        if let Some(mtime) = mtime {
            attr.mtime = match mtime {
                fuser::TimeOrNow::SpecificTime(v) => v,
                fuser::TimeOrNow::Now => std::time::SystemTime::now(),
            };
        }
        if let Some(ctime) = ctime {
            attr.ctime = ctime;
        }
        if let Some(crtime) = crtime {
            attr.crtime = crtime;
        }
        log::debug!("{attr:?}");
        reply.attr(&TTL, &attr);
    }

    fn mknod(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("dnid {dnid} name {name:?} mode {mode:#o} umask {umask:#o} rdev {rdev}");
        let _mtx = mtx_lock!(MTX);
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.mknod_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        reply.entry(&TTL, &stat2attr(&st), 0);
    }

    fn mkdir(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("dnid {dnid} name {name:?} mode {mode:#o} umask {umask:#o}");
        let _mtx = mtx_lock!(MTX);
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.mkdir_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        reply.entry(&TTL, &stat2attr(&st), 0);
    }

    fn unlink(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("dnid {dnid} name {name:?}");
        let _mtx = mtx_lock!(MTX);
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.lookup_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        if let Err(e) = self.ef.unlink(nid) {
            if let Some(node) = self.ef.get_node_mut(nid) {
                node.put();
            }
            reply.error(e2i(e));
            return;
        }
        reply.ok();
    }

    fn rmdir(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("dnid {dnid} name {name:?}");
        let _mtx = mtx_lock!(MTX);
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.lookup_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        if let Err(e) = self.ef.rmdir(nid) {
            if let Some(node) = self.ef.get_node_mut(nid) {
                node.put();
            }
            reply.error(e2i(e));
            return;
        }
        reply.ok();
    }

    fn rename(
        &mut self,
        req: &fuser::Request<'_>,
        old_dnid: u64,
        old_name: &std::ffi::OsStr,
        new_dnid: u64,
        new_name: &std::ffi::OsStr,
        flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!(
            "old_dnid {old_dnid} old_name {old_name:?} \
            new_dnid {new_dnid} new_name {new_name:?} flags {flags:#x}"
        );
        let _mtx = mtx_lock!(MTX);
        let Some(old_name) = old_name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let Some(new_name) = new_name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        if let Err(e) = self.ef.rename_at(old_dnid, old_name, new_dnid, new_name) {
            reply.error(e2i(e));
            return;
        }
        reply.ok();
    }

    fn open(&mut self, req: &fuser::Request<'_>, nid: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} flags {flags:#x}");
        let _mtx = mtx_lock!(MTX);
        let Some(node) = self.ef.get_node(nid) else {
            reply.error(libc::ENOENT);
            return;
        };
        assert_eq!(node.get_nid(), nid);
        get_node_mut!(self.ef, nid).get(); // put on release

        // https://docs.rs/fuser/latest/fuser/trait.Filesystem.html#method.open
        // says "Open flags (with the exception of O_CREAT, O_EXCL, O_NOCTTY and O_TRUNC)
        // are available in flags.".
        if (flags & libc::O_TRUNC) != 0 {
            if let Err(e) = self.ef.truncate(nid, 0, true) {
                reply.error(e2i(e));
                return;
            }
        }
        self.total_open += 1;
        reply.opened(nid, fuser::consts::FOPEN_KEEP_CACHE);
    }

    fn read(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!(
            "nid {nid} fh {fh} offset {offset} size {size} flags {flags:#x} \
            lock_owner {lock_owner:?}"
        );
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        let mut buf = vec![0; size.try_into().unwrap()];
        let bytes = match self.ef.pread(nid, &mut buf, offset.try_into().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        reply.data(&buf[..bytes.try_into().unwrap()]);
    }

    fn write(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!(
            "nid {nid} fh {fh} offset {offset} size {} write_flags {write_flags:#x} \
            flags {flags:#x} lock_owner {lock_owner:?}",
            data.len()
        );
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        let bytes = match self.ef.pwrite(nid, data, offset.try_into().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        reply.written(bytes.try_into().unwrap());
    }

    fn flush(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} fh {fh} lock_owner {lock_owner:?}");
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        if let Err(e) = self.ef.flush_node(nid) {
            reply.error(e2i(e));
            return;
        }
        reply.ok();
    }

    fn release(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        flags: i32,
        lock_owner: Option<u64>,
        flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!(
            "nid {nid} fh {fh} flags {flags:#x} flush {flush} \
            lock_owner {lock_owner:?}"
        );
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        if let Err(e) = self.ef.flush_node(nid) {
            reply.error(e2i(e));
            return;
        }
        assert!(self.total_open > 0);
        self.total_open -= 1;
        get_node_mut!(self.ef, nid).put();
        reply.ok();
    }

    fn fsync(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} fh {fh} datasync {datasync}");
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        if let Err(e) = self.ef.flush_nodes() {
            reply.error(e2i(e));
            return;
        }
        if let Err(e) = self.ef.flush() {
            reply.error(e2i(e));
            return;
        }
        // libexfat's fsync is to fsync device fd, not to fsync this nid...
        if let Err(e) = self.ef.fsync() {
            reply.error(e2i(e));
            return;
        }
        reply.ok();
    }

    fn opendir(&mut self, req: &fuser::Request<'_>, nid: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} flags {flags:#x}");
        let _mtx = mtx_lock!(MTX);
        let Some(node) = self.ef.get_node(nid) else {
            reply.error(libc::ENOENT);
            return;
        };
        assert_eq!(node.get_nid(), nid);
        get_node_mut!(self.ef, nid).get(); // put on releasedir
        self.total_open += 1;
        reply.opened(nid, fuser::consts::FOPEN_KEEP_CACHE);
    }

    fn readdir(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("dnid {dnid} fh {fh} offset {offset}");
        let _mtx = mtx_lock!(MTX);
        assert_eq!(dnid, fh);
        let Some(dnode) = self.ef.get_node(dnid) else {
            reply.error(libc::ENOENT);
            return;
        };
        if !dnode.is_directory() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let mut offset = offset;
        if offset < 1 {
            if reply.add(dnode.get_nid(), 1, fuser::FileType::Directory, ".") {
                reply.ok();
                return;
            }
            offset += 1;
        }
        if offset < 2 {
            if reply.add(dnode.get_pnid(), 2, fuser::FileType::Directory, "..") {
                reply.ok();
                return;
            }
            offset += 1;
        }

        let mut c = match self.ef.opendir_cursor(dnid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        let mut next = 3;
        loop {
            let nid = match self.ef.readdir_cursor(&mut c) {
                Ok(v) => v,
                Err(e) => {
                    if let libexfat::Error::Errno(e) = e {
                        if e == nix::errno::Errno::ENOENT {
                            break;
                        }
                    }
                    self.ef.closedir_cursor(c);
                    reply.error(e2i(e));
                    return;
                }
            };
            if offset < next {
                let node = get_node!(self.ef, nid);
                let st = match self.ef.stat(nid) {
                    Ok(v) => v,
                    Err(e) => {
                        get_node_mut!(self.ef, nid).put();
                        self.ef.closedir_cursor(c);
                        reply.error(e2i(e));
                        return;
                    }
                };
                if reply.add(
                    st.st_ino,
                    next,
                    crate::util::mode2kind(st.st_mode),
                    node.get_name(),
                ) {
                    get_node_mut!(self.ef, nid).put();
                    break;
                }
                offset += 1;
            }
            get_node_mut!(self.ef, nid).put();
            next += 1;
        }
        self.ef.closedir_cursor(c);
        reply.ok();
    }

    fn releasedir(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} fh {fh} flags {flags:#x}");
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        assert!(self.total_open > 0);
        self.total_open -= 1;
        get_node_mut!(self.ef, nid).put();
        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} fh {fh} datasync {datasync}");
        self.fsync(req, nid, fh, datasync, reply);
    }

    fn statfs(&mut self, req: &fuser::Request<'_>, nid: u64, reply: fuser::ReplyStatfs) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid}");
        let _mtx = mtx_lock!(MTX);
        match self.ef.statfs() {
            Ok(v) => reply.statfs(
                v.f_blocks,
                v.f_bfree,
                v.f_bavail,
                v.f_files,
                v.f_ffree,
                v.f_bsize,
                v.f_namelen,
                v.f_frsize,
            ),
            Err(e) => reply.error(e2i(e)),
        }
    }

    // https://docs.rs/fuser/latest/fuser/trait.Filesystem.html
    // If the default_permissions mount option is given, this method is not called.
    fn access(&mut self, req: &fuser::Request<'_>, nid: u64, mask: i32, reply: fuser::ReplyEmpty) {
        debug_req!(req, self.debug > 1);
        log::debug!("nid {nid} mask {mask:#o}");
        let _mtx = mtx_lock!(MTX);
        reply.ok();
        panic!("access");
    }

    fn create(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!(
            "dnid {dnid} name {name:?} mode {mode:#o} umask {umask:#o} \
            flags {flags:#x}"
        );
        let _mtx = mtx_lock!(MTX);
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.mknod_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        get_node_mut!(self.ef, nid).get(); // put on release
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e2i(e));
                return;
            }
        };
        self.total_open += 1;
        reply.created(&TTL, &stat2attr(&st), 0, nid, 0);
    }

    // Not supported on FreeBSD (see fuse_vnop_ioctl()).
    fn ioctl(
        &mut self,
        req: &fuser::Request<'_>,
        nid: u64,
        fh: u64,
        flags: u32,
        cmd: u32,
        in_data: &[u8],
        out_size: u32,
        reply: fuser::ReplyIoctl,
    ) {
        debug_req!(req, self.debug > 1);
        log::debug!(
            "nid {nid} fh {fh} flags {flags:#x} cmd {cmd:#x} in_data {in_data:?} \
            out_size {out_size}"
        );
        let _mtx = mtx_lock!(MTX);
        assert_eq!(nid, fh);
        if u64::from(cmd) == libexfat::ctl::CTL_NIDPRUNE_ENCODE {
            log::debug!("CTL_NIDPRUNE");
            assert!(self.total_open > 0); // fd for this nid
            let x = self.total_open - 1;
            if x > 0 {
                log::error!("{x} pending open file");
                reply.error(libc::EBUSY);
                return;
            }
            let t = match self.ef.prune_node(nid) {
                Ok(v) => v,
                Err(e) => {
                    reply.error(e2i(e));
                    return;
                }
            };
            let mut b = [0; 16];
            byteorder::BigEndian::write_u64_into(&[t.0.try_into().unwrap()], &mut b[..8]);
            byteorder::BigEndian::write_u64_into(&[t.1.try_into().unwrap()], &mut b[8..]);
            reply.ioctl(0, &b);
        } else {
            log::error!("invalid ioctl command {cmd:#x}");
            reply.error(libc::EINVAL);
        }
    }
}
