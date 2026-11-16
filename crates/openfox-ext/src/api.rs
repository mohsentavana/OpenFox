use wasmtime::Caller;

/// The string ABI between host and guest:
///
/// - the guest exports `alloc(len: i32) -> i32`
/// - the host writes bytes through that allocation
/// - the guest reads its own memory (it knows the layout it returned)
/// - host-bound strings are `(ptr: i32, len: i32)` pairs
///
/// Nothing is copied through global state, nothing is guessed: the pointer
/// the guest returned is the pointer the host writes to.
pub fn read_string<T>(caller: &mut Caller<'_, T>, ptr: i32, len: i32) -> String {
    if ptr <= 0 || len <= 0 {
        return String::new();
    }
    let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
        return String::new();
    };
    let start = ptr as usize;
    let end = start + len as usize;
    let data = memory.data(&caller);
    if end > data.len() {
        return String::new();
    }
    String::from_utf8_lossy(&data[start..end]).into_owned()
}
