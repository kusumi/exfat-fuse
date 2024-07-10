use crate::util;

use crate::get_mut_node;
use crate::get_node;
use crate::ExfatFuse;

macro_rules! debug_req {
    ($req:expr, $debug:expr) => {
        if $debug {
            log::debug!("{:?}", $req);
        }
    };
}

// libexfat (both relan/exfat and Rust) isn't thread-safe.
// relan/exfat uses libfuse in single-thread mode (-s option).
lazy_static! {
    static ref MTX: std::sync::Mutex<i32> = std::sync::Mutex::new(0);
}

const TTL: std::time::Duration = std::time::Duration::from_secs(1);

fn stat2attr(st: &libexfat::exfat::ExfatStat) -> fuser::FileAttr {
    let attr = util::stat2attr(st);
    log::debug!("{attr:?}");
    attr
}

impl fuser::Filesystem for ExfatFuse {
    fn init(
        &mut self,
        req: &fuser::Request<'_>,
        config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("config {config:?}");
        let _mtx = MTX.lock().unwrap();
        // mark super block as dirty; failure isn't a big deal
        if let Err(e) = self.ef.soil_super_block() {
            return Err(e as i32);
        }
        Ok(())
    }

    fn destroy(&mut self) {
        log::debug!("destroy");
        let _mtx = MTX.lock().unwrap();
        self.ef.unmount().unwrap();
    }

    fn lookup(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("dnid {dnid} name {name:?}");
        let _mtx = MTX.lock().unwrap();
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.lookup_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        get_mut_node!(self.ef, nid).put();
        reply.entry(&TTL, &stat2attr(&st), 0);
    }

    fn getattr(&mut self, req: &fuser::Request<'_>, nid: u64, reply: fuser::ReplyAttr) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid}");
        let _mtx = MTX.lock().unwrap();
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        reply.attr(&TTL, &stat2attr(&st));
    }

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
        debug_req!(req, self.debug && self.verbose);
        if self.debug {
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
        let _mtx = MTX.lock().unwrap();
        if let Some(fh) = fh {
            assert_eq!(nid, fh);
        }
        let mut st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        if let Some(mode) = mode {
            let valid_mode_mask =
                libc::S_IFREG | libc::S_IFDIR | libc::S_IRWXU | libc::S_IRWXG | libc::S_IRWXO;
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
            get_mut_node!(self.ef, nid).get();
            if let Err(e) = self.ef.truncate(nid, size, true) {
                if self.ef.flush_node(nid).is_err() {
                    // ignore this error
                }
                get_mut_node!(self.ef, nid).put();
                reply.error(e as i32);
                return;
            }
            if let Err(e) = self.ef.flush_node(nid) {
                get_mut_node!(self.ef, nid).put();
                reply.error(e as i32);
                return;
            }
            get_mut_node!(self.ef, nid).put();
            // truncate has updated mtime
            st = match self.ef.stat(nid) {
                Ok(v) => v,
                Err(e) => {
                    reply.error(e as i32);
                    return;
                }
            };
            st.st_size = size;
        }
        let mut attr = util::stat2attr(&st);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("dnid {dnid} name {name:?} mode {mode:#o} umask {umask:#o} rdev {rdev}");
        let _mtx = MTX.lock().unwrap();
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.mknod_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("dnid {dnid} name {name:?} mode {mode:#o} umask {umask:#o}");
        let _mtx = MTX.lock().unwrap();
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.mkdir_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("dnid {dnid} name {name:?}");
        let _mtx = MTX.lock().unwrap();
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.lookup_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        if let Err(e) = self.ef.unlink(nid) {
            if let Some(node) = self.ef.get_mut_node(nid) {
                node.put();
            }
            reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("dnid {dnid} name {name:?}");
        let _mtx = MTX.lock().unwrap();
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.lookup_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        if let Err(e) = self.ef.rmdir(nid) {
            if let Some(node) = self.ef.get_mut_node(nid) {
                node.put();
            }
            reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!(
            "old_dnid {old_dnid} old_name {old_name:?} \
            new_dnid {new_dnid} new_name {new_name:?} flags {flags:#x}"
        );
        let _mtx = MTX.lock().unwrap();
        let Some(old_name) = old_name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let Some(new_name) = new_name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        if let Err(e) = self.ef.rename_at(old_dnid, old_name, new_dnid, new_name) {
            reply.error(e as i32);
            return;
        }
        reply.ok();
    }

    fn open(&mut self, req: &fuser::Request<'_>, nid: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} flags {flags:#x}");
        let _mtx = MTX.lock().unwrap();
        let Some(node) = self.ef.get_node(nid) else {
            reply.error(libc::ENOENT);
            return;
        };
        assert_eq!(node.get_nid(), nid);
        get_mut_node!(self.ef, nid).get(); // put on release

        // https://docs.rs/fuser/latest/fuser/trait.Filesystem.html#method.open
        // says "Open flags (with the exception of O_CREAT, O_EXCL, O_NOCTTY and O_TRUNC)
        // are available in flags.".
        if (flags & libc::O_TRUNC) != 0 {
            if let Err(e) = self.ef.truncate(nid, 0, true) {
                reply.error(e as i32);
                return;
            }
        }
        reply.opened(nid, 0);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!(
            "nid {nid} fh {fh} offset {offset} size {size} flags {flags:#x} \
            lock_owner {lock_owner:?}"
        );
        let _mtx = MTX.lock().unwrap();
        assert_eq!(nid, fh);
        let mut buf = vec![0; size.try_into().unwrap()];
        let bytes = match self.ef.pread(nid, &mut buf, offset.try_into().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!(
            "nid {nid} fh {fh} offset {offset} size {} write_flags {write_flags:#x} \
            flags {flags:#x} lock_owner {lock_owner:?}",
            data.len()
        );
        let _mtx = MTX.lock().unwrap();
        assert_eq!(nid, fh);
        let bytes = match self.ef.pwrite(nid, data, offset.try_into().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} fh {fh} lock_owner {lock_owner:?}");
        let _mtx = MTX.lock().unwrap();
        assert_eq!(nid, fh);
        if let Err(e) = self.ef.flush_node(nid) {
            reply.error(e as i32);
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!(
            "nid {nid} fh {fh} flags {flags:#x} flush {flush} \
            lock_owner {lock_owner:?}"
        );
        let _mtx = MTX.lock().unwrap();
        assert_eq!(nid, fh);
        if let Err(e) = self.ef.flush_node(nid) {
            reply.error(e as i32);
            return;
        }
        get_mut_node!(self.ef, nid).put();
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} fh {fh} datasync {datasync}");
        let _mtx = MTX.lock().unwrap();
        assert_eq!(nid, fh);
        if let Err(e) = self.ef.flush_nodes() {
            reply.error(e as i32);
            return;
        }
        if let Err(e) = self.ef.flush() {
            reply.error(e as i32);
            return;
        }
        // libexfat's fsync is to fsync device fd, not to fsync this nid...
        if let Err(e) = self.ef.fsync() {
            reply.error(e as i32);
            return;
        }
        reply.ok();
    }

    fn opendir(&mut self, req: &fuser::Request<'_>, nid: u64, flags: i32, reply: fuser::ReplyOpen) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} flags {flags:#x}");
        let _mtx = MTX.lock().unwrap();
        let Some(node) = self.ef.get_node(nid) else {
            reply.error(libc::ENOENT);
            return;
        };
        assert_eq!(node.get_nid(), nid);
        get_mut_node!(self.ef, nid).get(); // put on releasedir
        reply.opened(nid, 0);
    }

    fn readdir(
        &mut self,
        req: &fuser::Request<'_>,
        dnid: u64,
        fh: u64,
        offset: i64,
        mut reply: fuser::ReplyDirectory,
    ) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("dnid {dnid} fh {fh} offset {offset}");
        let _mtx = MTX.lock().unwrap();
        assert_eq!(dnid, fh);
        let node = get_node!(self.ef, dnid);
        if !node.is_directory() {
            reply.error(libc::ENOTDIR);
            return;
        }

        let mut offset = offset;
        if offset < 1 {
            if reply.add(node.get_nid(), 1, fuser::FileType::Directory, ".") {
                reply.ok();
                return;
            }
            offset += 1;
        }
        if offset < 2 {
            if reply.add(node.get_pnid(), 2, fuser::FileType::Directory, "..") {
                reply.ok();
                return;
            }
            offset += 1;
        }

        let mut c = match self.ef.opendir_cursor(dnid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        let mut next = 3;
        loop {
            let nid = match self.ef.readdir_cursor(&mut c) {
                Ok(v) => v,
                Err(nix::errno::Errno::ENOENT) => break,
                Err(e) => {
                    self.ef.closedir_cursor(c);
                    reply.error(e as i32);
                    return;
                }
            };
            if offset < next {
                let node = get_node!(self.ef, nid);
                let st = match self.ef.stat(nid) {
                    Ok(v) => v,
                    Err(e) => {
                        get_mut_node!(self.ef, nid).put();
                        self.ef.closedir_cursor(c);
                        reply.error(e as i32);
                        return;
                    }
                };
                if reply.add(
                    st.st_ino,
                    next,
                    util::mode2kind(st.st_mode),
                    node.get_name(),
                ) {
                    get_mut_node!(self.ef, nid).put();
                    break;
                }
                offset += 1;
            }
            get_mut_node!(self.ef, nid).put();
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} fh {fh} flags {flags:#x}");
        let _mtx = MTX.lock().unwrap();
        assert_eq!(nid, fh);
        get_mut_node!(self.ef, nid).put();
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} fh {fh} datasync {datasync}");
        self.fsync(req, nid, fh, datasync, reply);
    }

    fn statfs(&mut self, req: &fuser::Request<'_>, nid: u64, reply: fuser::ReplyStatfs) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid}");
        let _mtx = MTX.lock().unwrap();
        let sfs = self.ef.statfs();
        reply.statfs(
            sfs.f_blocks,
            sfs.f_bfree,
            sfs.f_bavail,
            sfs.f_files,
            sfs.f_ffree,
            sfs.f_bsize,
            sfs.f_namelen,
            sfs.f_frsize,
        );
    }

    // https://docs.rs/fuser/latest/fuser/trait.Filesystem.html
    // If the default_permissions mount option is given, this method is not called.
    fn access(&mut self, req: &fuser::Request<'_>, nid: u64, mask: i32, reply: fuser::ReplyEmpty) {
        debug_req!(req, self.debug && self.verbose);
        log::debug!("nid {nid} mask {mask:#o}");
        let _mtx = MTX.lock().unwrap();
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
        debug_req!(req, self.debug && self.verbose);
        log::debug!(
            "dnid {dnid} name {name:?} mode {mode:#o} umask {umask:#o} \
            flags {flags:#x}"
        );
        let _mtx = MTX.lock().unwrap();
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let nid = match self.ef.mknod_at(dnid, name) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        get_mut_node!(self.ef, nid).get(); // put on release
        let st = match self.ef.stat(nid) {
            Ok(v) => v,
            Err(e) => {
                reply.error(e as i32);
                return;
            }
        };
        reply.created(&TTL, &stat2attr(&st), 0, nid, 0);
    }
}
