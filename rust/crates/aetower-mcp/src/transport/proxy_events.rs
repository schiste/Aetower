//! macOS proxy readiness and parent-exit notifications, without a polling tick.

use std::{
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    time::Duration,
};

pub(super) struct Readiness {
    queue: OwnedFd,
    stdin_fd: i32,
    stdin_enabled: bool,
}

fn event(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent {
        ident,
        filter,
        flags,
        fflags,
        data: 0,
        udata: std::ptr::null_mut(),
    }
}

impl Readiness {
    pub(super) fn new(stdin_fd: i32, socket_fd: i32, parent_pid: libc::pid_t) -> io::Result<Self> {
        if stdin_fd < 0 || socket_fd < 0 || parent_pid <= 1 {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // The new descriptor is owned immediately, including on setup failure.
        let queue = unsafe { OwnedFd::from_raw_fd(fd) };
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
        let changes = [
            event(stdin_fd as usize, libc::EVFILT_READ, libc::EV_ADD, 0),
            event(socket_fd as usize, libc::EVFILT_READ, libc::EV_ADD, 0),
            event(
                parent_pid as usize,
                libc::EVFILT_PROC,
                libc::EV_ADD | libc::EV_ONESHOT,
                libc::NOTE_EXIT,
            ),
        ];
        Self::change(&queue, &changes)?;
        Ok(Self {
            queue,
            stdin_fd,
            stdin_enabled: true,
        })
    }

    fn change(queue: &OwnedFd, changes: &[libc::kevent]) -> io::Result<()> {
        // With no output slots a registration error is returned directly;
        // closing the queue rolls back any earlier registrations on failure.
        let result = unsafe {
            libc::kevent(
                queue.as_raw_fd(),
                changes.as_ptr(),
                changes.len() as i32,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// `None` means the watched parent exited. Readable data/EOF is translated
    /// into the same poll bits consumed by the unchanged forwarding loop.
    pub(super) fn wait(
        &mut self,
        fds: &mut [libc::pollfd; 2],
        stdin_open: bool,
        remaining: Duration,
    ) -> io::Result<Option<i32>> {
        for fd in fds.iter_mut() {
            fd.revents = 0;
        }
        if !stdin_open && self.stdin_enabled {
            Self::change(
                &self.queue,
                &[event(
                    self.stdin_fd as usize,
                    libc::EVFILT_READ,
                    libc::EV_DELETE,
                    0,
                )],
            )?;
            self.stdin_enabled = false;
        }
        let timeout = libc::timespec {
            tv_sec: remaining.as_secs() as libc::time_t,
            tv_nsec: remaining.subsec_nanos() as libc::c_long,
        };
        let mut events = unsafe { std::mem::zeroed::<[libc::kevent; 3]>() };
        let count = unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                std::ptr::null(),
                0,
                events.as_mut_ptr(),
                events.len() as i32,
                &timeout,
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        for e in &events[..count as usize] {
            if e.flags & libc::EV_ERROR != 0 {
                return Err(io::Error::from_raw_os_error(e.data as i32));
            }
            if e.filter == libc::EVFILT_PROC {
                return Ok(None);
            }
            if e.filter == libc::EVFILT_READ {
                for fd in fds.iter_mut() {
                    if e.ident == fd.fd as usize {
                        fd.revents |= libc::POLLIN;
                        if e.flags & libc::EV_EOF != 0 {
                            fd.revents |= libc::POLLHUP;
                        }
                    }
                }
            }
        }
        Ok(Some(fds.iter().filter(|fd| fd.revents != 0).count() as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
        process::{Command, Stdio},
    };

    fn fds(input: &UnixStream, socket: &UnixStream) -> [libc::pollfd; 2] {
        [
            libc::pollfd {
                fd: input.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: socket.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ]
    }

    #[test]
    fn idle_wait_reaches_deadline_without_readiness() {
        let (input, _writer) = UnixStream::pair().unwrap();
        let (socket, _peer) = UnixStream::pair().unwrap();
        let mut wait = Readiness::new(
            input.as_raw_fd(),
            socket.as_raw_fd(),
            std::process::id() as i32,
        )
        .unwrap();
        let mut ready = fds(&input, &socket);
        ready[0].revents = libc::POLLIN;
        assert_eq!(
            wait.wait(&mut ready, true, Duration::from_millis(5))
                .unwrap(),
            Some(0)
        );
    }

    #[test]
    fn closed_stdin_is_disabled_but_socket_data_and_eof_still_arrive() {
        let (input, writer) = UnixStream::pair().unwrap();
        let (mut socket, mut peer) = UnixStream::pair().unwrap();
        let mut wait = Readiness::new(
            input.as_raw_fd(),
            socket.as_raw_fd(),
            std::process::id() as i32,
        )
        .unwrap();
        drop(writer);
        assert_eq!(
            wait.wait(&mut fds(&input, &socket), false, Duration::from_millis(5))
                .unwrap(),
            Some(0)
        );
        peer.write_all(b"response after stdin EOF").unwrap();
        peer.shutdown(std::net::Shutdown::Write).unwrap();
        let mut ready = fds(&input, &socket);
        assert_eq!(
            wait.wait(&mut ready, false, Duration::from_secs(2))
                .unwrap(),
            Some(1)
        );
        assert_ne!(ready[1].revents & libc::POLLIN, 0);
        let mut response = Vec::new();
        socket.read_to_end(&mut response).unwrap();
        assert_eq!(response, b"response after stdin EOF");
    }

    #[test]
    fn watched_process_exit_wakes_the_wait() {
        let (input, _writer) = UnixStream::pair().unwrap();
        let (socket, _peer) = UnixStream::pair().unwrap();
        let mut child = Command::new("/bin/sh")
            .args(["-c", "read fixture"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut wait =
            Readiness::new(input.as_raw_fd(), socket.as_raw_fd(), child.id() as i32).unwrap();
        drop(child.stdin.take());
        let result = wait.wait(&mut fds(&input, &socket), true, Duration::from_secs(2));
        child.wait().unwrap();
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn unsupported_parent_watch_falls_back_to_data_readiness() {
        let (mut input, mut writer) = UnixStream::pair().unwrap();
        let (socket, _peer) = UnixStream::pair().unwrap();
        let mut wait = super::super::ProxyWaiter::new(input.as_raw_fd(), socket.as_raw_fd(), -1);
        writer.write_all(b"fallback payload").unwrap();
        let mut ready = fds(&input, &socket);
        assert_eq!(
            wait.wait(&mut ready, true, Duration::from_secs(2)).unwrap(),
            Some(1)
        );
        assert_ne!(ready[0].revents & libc::POLLIN, 0);
        let mut bytes = [0; 16];
        input.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"fallback payload");
    }
}
