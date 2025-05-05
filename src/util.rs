pub(crate) fn get_home_path() -> crate::Result<String> {
    Ok(home::home_dir()
        .ok_or(nix::errno::Errno::ENOENT)?
        .into_os_string()
        .into_string()
        .unwrap())
}

pub(crate) fn stat2attr(st: &libexfat::exfat::Stat) -> fuser::FileAttr {
    let mtime = libfs::time::unix2system(st.st_mtime);
    fuser::FileAttr {
        ino: st.st_ino,
        size: st.st_size,
        blocks: st.st_blocks,
        atime: libfs::time::unix2system(st.st_atime),
        mtime,
        ctime: mtime,
        crtime: mtime,
        kind: mode2kind(st.st_mode),
        perm: (st.st_mode & 0o777).try_into().unwrap(),
        nlink: st.st_nlink,
        uid: st.st_uid,
        gid: st.st_gid,
        rdev: st.st_rdev,
        blksize: st.st_blksize,
        flags: 0,
    }
}

pub(crate) fn mode2kind(mode: libexfat::exfat::StatMode) -> fuser::FileType {
    match mode & libc::S_IFMT {
        libc::S_IFDIR => fuser::FileType::Directory,
        libc::S_IFREG => fuser::FileType::RegularFile,
        _ => panic!("{mode:o}"),
    }
}
