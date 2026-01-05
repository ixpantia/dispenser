use std::{
    net::SocketAddrV4,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub struct AtomicOptionSocketAddrV4(AtomicU64);

/// We use this value as our `None` variant since this bit pattern
/// is unrepresentable for valid socket addresses.
///
/// Bit layout of encoded values:
/// - Bits 0-15:  port (16 bits)
/// - Bits 16-47: IPv4 address (32 bits)
/// - Bits 48-63: unused (always 0 for valid addresses)
///
/// Since the upper 16 bits are always 0 for valid addresses,
/// `u64::MAX` (all bits set) can never represent a valid address,
/// making it safe to use as the `None` sentinel value.
const NONE_VALUE: u64 = u64::MAX;

#[inline]
fn socket_addr_v4_to_u64(val: Option<SocketAddrV4>) -> u64 {
    match val {
        None => NONE_VALUE,
        Some(val) => {
            let ip: u32 = val.ip().to_bits();
            let port: u16 = val.port();
            ((ip as u64) << 16) | (port as u64)
        }
    }
}

#[inline]
fn u64_to_socket_addr_v4(val: u64) -> Option<SocketAddrV4> {
    if val == NONE_VALUE {
        return None;
    }
    let ip = std::net::Ipv4Addr::from_bits((val >> 16) as u32);
    let port = (val & 0xFFFF) as u16;
    Some(SocketAddrV4::new(ip, port))
}

impl Default for AtomicOptionSocketAddrV4 {
    fn default() -> Self {
        Self::new(None)
    }
}

impl AtomicOptionSocketAddrV4 {
    pub fn new(socket_addr: Option<SocketAddrV4>) -> Self {
        AtomicOptionSocketAddrV4(AtomicU64::new(socket_addr_v4_to_u64(socket_addr)))
    }

    #[must_use]
    pub fn load(&self, order: Ordering) -> Option<SocketAddrV4> {
        u64_to_socket_addr_v4(self.0.load(order))
    }

    pub fn store(&self, val: Option<SocketAddrV4>, order: Ordering) {
        self.0.store(socket_addr_v4_to_u64(val), order)
    }

    #[must_use]
    pub fn swap(&self, val: Option<SocketAddrV4>, order: Ordering) -> Option<SocketAddrV4> {
        u64_to_socket_addr_v4(self.0.swap(socket_addr_v4_to_u64(val), order))
    }

    #[must_use]
    pub fn compare_exchange(
        &self,
        current: Option<SocketAddrV4>,
        new: Option<SocketAddrV4>,
        success: Ordering,
        failure: Ordering,
    ) -> Result<Option<SocketAddrV4>, Option<SocketAddrV4>> {
        self.0
            .compare_exchange(
                socket_addr_v4_to_u64(current),
                socket_addr_v4_to_u64(new),
                success,
                failure,
            )
            .map(u64_to_socket_addr_v4)
            .map_err(u64_to_socket_addr_v4)
    }

    /// Weaker version of `compare_exchange` that may spuriously fail.
    /// Preferred for use in CAS loops due to better performance on some platforms.
    #[must_use]
    pub fn compare_exchange_weak(
        &self,
        current: Option<SocketAddrV4>,
        new: Option<SocketAddrV4>,
        success: Ordering,
        failure: Ordering,
    ) -> Result<Option<SocketAddrV4>, Option<SocketAddrV4>> {
        self.0
            .compare_exchange_weak(
                socket_addr_v4_to_u64(current),
                socket_addr_v4_to_u64(new),
                success,
                failure,
            )
            .map(u64_to_socket_addr_v4)
            .map_err(u64_to_socket_addr_v4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::atomic::Ordering::SeqCst;

    #[test]
    fn test_roundtrip_some() {
        let addr = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 8080);
        let encoded = socket_addr_v4_to_u64(Some(addr));
        let decoded = u64_to_socket_addr_v4(encoded);
        assert_eq!(decoded, Some(addr));
    }

    #[test]
    fn test_roundtrip_none() {
        let encoded = socket_addr_v4_to_u64(None);
        let decoded = u64_to_socket_addr_v4(encoded);
        assert_eq!(decoded, None);
    }

    #[test]
    fn test_edge_case_min() {
        let min = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0);
        assert_eq!(
            u64_to_socket_addr_v4(socket_addr_v4_to_u64(Some(min))),
            Some(min)
        );
    }

    #[test]
    fn test_edge_case_max() {
        let max = SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 65535);
        assert_eq!(
            u64_to_socket_addr_v4(socket_addr_v4_to_u64(Some(max))),
            Some(max)
        );
    }

    #[test]
    fn test_atomic_new_and_load() {
        let addr = SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 443);
        let atomic = AtomicOptionSocketAddrV4::new(Some(addr));
        assert_eq!(atomic.load(SeqCst), Some(addr));
    }

    #[test]
    fn test_atomic_new_none() {
        let atomic = AtomicOptionSocketAddrV4::new(None);
        assert_eq!(atomic.load(SeqCst), None);
    }

    #[test]
    fn test_atomic_default() {
        let atomic = AtomicOptionSocketAddrV4::default();
        assert_eq!(atomic.load(SeqCst), None);
    }

    #[test]
    fn test_atomic_store() {
        let atomic = AtomicOptionSocketAddrV4::new(None);
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8000);
        atomic.store(Some(addr), SeqCst);
        assert_eq!(atomic.load(SeqCst), Some(addr));

        atomic.store(None, SeqCst);
        assert_eq!(atomic.load(SeqCst), None);
    }

    #[test]
    fn test_atomic_swap() {
        let addr1 = SocketAddrV4::new(Ipv4Addr::new(192, 168, 0, 1), 80);
        let addr2 = SocketAddrV4::new(Ipv4Addr::new(10, 10, 10, 10), 443);

        let atomic = AtomicOptionSocketAddrV4::new(Some(addr1));
        let old = atomic.swap(Some(addr2), SeqCst);

        assert_eq!(old, Some(addr1));
        assert_eq!(atomic.load(SeqCst), Some(addr2));
    }

    #[test]
    fn test_atomic_compare_exchange_success() {
        let addr1 = SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 1234);
        let addr2 = SocketAddrV4::new(Ipv4Addr::new(5, 6, 7, 8), 5678);

        let atomic = AtomicOptionSocketAddrV4::new(Some(addr1));
        let result = atomic.compare_exchange(Some(addr1), Some(addr2), SeqCst, SeqCst);

        assert_eq!(result, Ok(Some(addr1)));
        assert_eq!(atomic.load(SeqCst), Some(addr2));
    }

    #[test]
    fn test_atomic_compare_exchange_failure() {
        let addr1 = SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 1234);
        let addr2 = SocketAddrV4::new(Ipv4Addr::new(5, 6, 7, 8), 5678);
        let addr3 = SocketAddrV4::new(Ipv4Addr::new(9, 10, 11, 12), 9012);

        let atomic = AtomicOptionSocketAddrV4::new(Some(addr1));
        let result = atomic.compare_exchange(Some(addr2), Some(addr3), SeqCst, SeqCst);

        assert_eq!(result, Err(Some(addr1)));
        assert_eq!(atomic.load(SeqCst), Some(addr1));
    }

    #[test]
    fn test_atomic_compare_exchange_with_none() {
        let addr = SocketAddrV4::new(Ipv4Addr::new(172, 16, 0, 1), 9999);

        let atomic = AtomicOptionSocketAddrV4::new(None);
        let result = atomic.compare_exchange(None, Some(addr), SeqCst, SeqCst);

        assert_eq!(result, Ok(None));
        assert_eq!(atomic.load(SeqCst), Some(addr));
    }

    #[test]
    fn test_none_value_is_not_valid_address() {
        // Ensure that the max valid encoded address doesn't collide with NONE_VALUE
        let max_addr = SocketAddrV4::new(Ipv4Addr::new(255, 255, 255, 255), 65535);
        let encoded = socket_addr_v4_to_u64(Some(max_addr));
        assert_ne!(encoded, NONE_VALUE);
    }
}
