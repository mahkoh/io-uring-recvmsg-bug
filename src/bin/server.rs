#![expect(clippy::len_zero, clippy::needless_late_init)]

use {
    io_uring::{IoUring, cqueue, opcode::RecvMsg, squeue, types::Fd},
    std::{
        fs,
        mem::MaybeUninit,
        os::{fd::AsRawFd, unix::net::UnixListener},
        ptr,
    },
    uapi::{MsghdrMut, OwnedFd, c, sockaddr_none_mut},
};

const USE_IO_URING: bool = true;

fn main() {
    let mut data_buf = [0u8; 24];
    let mut cmsg_buf = [0u8; 128];
    let _ = fs::remove_file("socket");
    let socket = UnixListener::bind("socket").unwrap();
    let (socket, _) = socket.accept().unwrap();
    if !USE_IO_URING {
        const SO_INQ: i32 = 84;
        uapi::setsockopt(socket.as_raw_fd(), c::SOL_SOCKET, SO_INQ, &1i32).unwrap();
    }
    let mut uring = IoUring::<squeue::Entry, cqueue::Entry>::builder()
        .setup_single_issuer()
        .setup_defer_taskrun()
        .build(32)
        .unwrap();
    let mut fds = 0;
    loop {
        data_buf.fill(0);
        let cmsg_orig;
        if USE_IO_URING {
            let datas = [&data_buf[..]];
            let mut msghdr = c::msghdr {
                msg_name: ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: datas.as_ptr() as *mut _,
                msg_iovlen: 1,
                msg_control: cmsg_buf.as_mut_ptr() as _,
                msg_controllen: cmsg_buf.len() as _,
                msg_flags: 0,
            };
            unsafe {
                uring
                    .submission()
                    .push(&RecvMsg::new(Fd(socket.as_raw_fd()), &mut msghdr).build())
                    .unwrap();
            }
            uring.submit_and_wait(1).unwrap();
            let mut completions = [MaybeUninit::uninit()];
            let completions = uring.completion().fill(&mut completions);
            for completion in completions {
                assert!(completion.result() > 0);
            }
            cmsg_orig = &cmsg_buf[..msghdr.msg_controllen as usize];
        } else {
            let mut msghdr = MsghdrMut {
                iov: &mut [&mut data_buf[..]][..],
                control: Some(&mut cmsg_buf[..]),
                name: sockaddr_none_mut(),
                flags: 0,
            };
            let (_, _, cmsg) = uapi::recvmsg(socket.as_raw_fd(), &mut msghdr, 0).unwrap();
            cmsg_orig = cmsg;
        }
        let mut have_non_rights = false;
        let mut is_corrupted = false;
        'check_cmsg: {
            let mut cmsg = cmsg_orig;
            while cmsg.len() > 0 {
                if have_non_rights {
                    // if the SCM_INQ cmsg is not the last cmsg, it's probably corrupted
                    is_corrupted = true;
                }
                let (_, hdr, data) = uapi::cmsg_read(&mut cmsg).unwrap();
                if (hdr.cmsg_level, hdr.cmsg_type) == (c::SOL_SOCKET, c::SCM_RIGHTS) {
                    for fd in uapi::pod_iter::<c::c_int, _>(data).unwrap() {
                        let _ = OwnedFd::new(fd);
                        fds += 1;
                    }
                } else {
                    have_non_rights = true;
                }
            }
            if !is_corrupted {
                break 'check_cmsg;
            }
            let mut cmsg = cmsg_orig;
            while cmsg.len() > 0 {
                let (_, hdr, data) = uapi::cmsg_read(&mut cmsg).unwrap();
                dbg!((hdr, data.len()));
            }
            eprintln!("---------");
        }
        let num_end = data_buf.iter().filter(|b| **b == 1).count();
        assert!(fds >= num_end);
        fds -= num_end;
    }
}
