exfat-fuse ([v0.2.2](https://github.com/kusumi/exfat-fuse/releases/tag/v0.2.2))
========

## About

Rust version of [https://github.com/relan/exfat/tree/master/fuse](https://github.com/relan/exfat/tree/master/fuse)

## Supported platforms

Linux

## Requirements

Rust 1.81.0 or newer

## Build

    $ make

## Install

    $ make install

## Uninstall

    $ make uninstall

## Usage

    $ ./target/release/exfat-fuse
    FUSE exfat 1.4.0 (fuser)
    Usage: ./target/release/exfat-fuse [options] <device> <directory>
    
    Options:
            --allow_other   Allow all users to access files on this filesystem. By
                            default access is restricted to the user who mounted
                            it.
            --allow_root    Allow the root user to access this filesystem, in
                            addition to the user who mounted it.
            --auto_unmount  Automatically unmount when the mounting process exits.
                            AutoUnmount requires AllowOther or AllowRoot. If
                            AutoUnmount is set and neither Allow... is set, the
                            FUSE configuration must permit allow_other, otherwise
                            mounting will fail.
            --ro            Read-only filesystem
            --noexec        Dont allow execution of binaries.
            --noatime       Dont update inode access time.
            --dirsync       All modifications to directories will be done
                            synchronously.
            --sync          All I/O will be done synchronously.
            --umask <value> Set the umask (the bitmask of the permissions that are
                            not present, in octal). The default is 0.
            --dmask <value> Set the umask for directories only.
            --fmask <value> Set the umask for files only.
            --uid <value>   Set the owner for all files and directories. The
                            default is the owner of the current process.
            --gid <value>   Set the group for all files and directories. The
                            default is the group of the current process.
        -o <options>        relan/exfat compatible file system specific options.
        -d                  Enable env_logger logging and do not daemonize.
        -V, --version       Print version and copyright.
        -h, --help          Print usage.

## Bugs

open-unlink fails with EBUSY.

## License

[GPLv2](COPYING)

Copyright (C) 2010-  Andrew Nayenko

Copyright (C) 2024-  Tomohiro Kusumi
