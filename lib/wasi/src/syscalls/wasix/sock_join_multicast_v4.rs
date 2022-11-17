use super::*;
use crate::syscalls::*;

/// ### `sock_join_multicast_v4()`
/// Joins a particular multicast IPv4 group
///
/// ## Parameters
///
/// * `fd` - Socket descriptor
/// * `multiaddr` - Multicast group to joined
/// * `interface` - Interface that will join
pub fn sock_join_multicast_v4<M: MemorySize>(
    mut ctx: FunctionEnvMut<'_, WasiEnv>,
    sock: WasiFd,
    multiaddr: WasmPtr<__wasi_addr_ip4_t, M>,
    iface: WasmPtr<__wasi_addr_ip4_t, M>,
) -> Errno {
    debug!(
        "wasi[{}:{}]::sock_join_multicast_v4 (fd={})",
        ctx.data().pid(),
        ctx.data().tid(),
        sock
    );

    let env = ctx.data();
    let memory = env.memory_view(&ctx);
    let multiaddr = wasi_try!(crate::state::read_ip_v4(&memory, multiaddr));
    let iface = wasi_try!(crate::state::read_ip_v4(&memory, iface));
    wasi_try!(__asyncify(&mut ctx, None, move |ctx| async move {
        __sock_actor_mut(
            ctx,
            sock,
            Rights::empty(),
            move |socket| async move { socket.join_multicast_v4(multiaddr, iface).await }
        )
        .await
    }));
    Errno::Success
}