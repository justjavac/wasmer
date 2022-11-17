use super::*;
use crate::syscalls::*;

/// ### `sock_bind()`
/// Bind a socket
/// Note: This is similar to `bind` in POSIX using PF_INET
///
/// ## Parameters
///
/// * `fd` - File descriptor of the socket to be bind
/// * `addr` - Address to bind the socket to
pub fn sock_bind<M: MemorySize>(
    mut ctx: FunctionEnvMut<'_, WasiEnv>,
    sock: WasiFd,
    addr: WasmPtr<__wasi_addr_port_t, M>,
) -> Errno {
    debug!(
        "wasi[{}:{}]::sock_bind (fd={})",
        ctx.data().pid(),
        ctx.data().tid(),
        sock
    );

    let env = ctx.data();
    let memory = env.memory_view(&ctx);
    let addr = wasi_try!(crate::state::read_ip_port(&memory, addr));
    let addr = SocketAddr::new(addr.0, addr.1);
    let net = env.net();
    wasi_try!(__asyncify(&mut ctx, None, move |ctx| async move {
        __sock_upgrade(
            ctx,
            sock,
            Rights::SOCK_BIND,
            move |socket| async move { socket.bind(net, addr).await }
        )
        .await
    }));
    Errno::Success
}