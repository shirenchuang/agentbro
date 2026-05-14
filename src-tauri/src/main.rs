// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(unix)]
fn raise_file_descriptor_limit() {
    const TARGET_NOFILE_LIMIT: libc::rlim_t = 4096;

    let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    let limit = unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) != 0 {
            return;
        }
        limit.assume_init()
    };

    if limit.rlim_cur >= TARGET_NOFILE_LIMIT {
        return;
    }

    let desired = if limit.rlim_max == libc::RLIM_INFINITY {
        TARGET_NOFILE_LIMIT
    } else {
        TARGET_NOFILE_LIMIT.min(limit.rlim_max)
    };

    if desired <= limit.rlim_cur {
        return;
    }

    let new_limit = libc::rlimit {
        rlim_cur: desired,
        rlim_max: limit.rlim_max,
    };
    unsafe {
        libc::setrlimit(libc::RLIMIT_NOFILE, &new_limit);
    }
}

#[cfg(unix)]
fn close_inherited_file_descriptors() {
    let Ok(entries) = std::fs::read_dir("/dev/fd") else {
        return;
    };

    let fds: Vec<libc::c_int> = entries
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .parse::<libc::c_int>()
                .ok()
        })
        .filter(|fd| *fd > 2)
        .collect();

    for fd in fds {
        unsafe {
            libc::close(fd);
        }
    }
}

fn main() {
    #[cfg(unix)]
    {
        raise_file_descriptor_limit();
        close_inherited_file_descriptors();
    }

    agentbro_lib::run()
}
