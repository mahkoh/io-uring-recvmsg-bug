use {
    std::{
        mem::offset_of,
        os::{fd::AsRawFd, unix::net::UnixStream},
    },
    uapi::{Msghdr, msghdr_control_none_ref, sockaddr_none_ref},
};

fn main() {
    let fd = uapi::memfd_create("", 0).unwrap();

    let data1 = [0u8; 8];
    let mut data2 = [0u8; 24];
    *data2.last_mut().unwrap() = 1;
    #[repr(C)]
    struct Cmsg {
        length: usize,
        level: u32,
        type_: u32,
        fd: i32,
        padding: i32,
    }
    let cmsg = Cmsg {
        length: offset_of!(Cmsg, padding),
        level: 1, // SOL_SOCKET
        type_: 1, // SCM_RIGHTS
        fd: fd.as_raw_fd(),
        padding: 0,
    };

    let socket = UnixStream::connect("socket").unwrap();
    loop {
        let msghdr = Msghdr {
            iov: &[&data1[..]][..],
            control: msghdr_control_none_ref(),
            name: sockaddr_none_ref(),
        };
        uapi::sendmsg(socket.as_raw_fd(), &msghdr, 0).unwrap();
        let msghdr = Msghdr {
            iov: &[&data2[..]][..],
            control: Some(&cmsg),
            name: sockaddr_none_ref(),
        };
        uapi::sendmsg(socket.as_raw_fd(), &msghdr, 0).unwrap();
    }
}
