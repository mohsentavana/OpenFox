//! The minidump writer: the Chromium minidump format in ~180 honest
//! lines. Header, stream directory, and the four streams a crash report
//! actually needs: threads, modules, system info, exception.

use crate::CrashError;

pub const MINIDUMP_MAGIC: u32 = 0x504D_444D; // 'MDMP'
pub const STREAM_COUNT: u32 = 4;

/// Stream types we write. The full enum in the spec has dozens; a
/// browser crash report needs these four to be actionable.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum StreamType {
    ThreadList = 3,
    ModuleList = 4,
    SystemInfo = 7,
    Exception = 6,
}

impl StreamType {
    fn from_u32(value: u32) -> Option<StreamType> {
        match value {
            3 => Some(StreamType::ThreadList),
            4 => Some(StreamType::ModuleList),
            6 => Some(StreamType::Exception),
            7 => Some(StreamType::SystemInfo),
            _ => None,
        }
    }
}

pub struct ThreadInfo {
    pub id: u32,
    pub stack: Vec<u8>,
    pub context_rva_data: Vec<u8>,
}

pub struct ModuleInfo {
    pub base: u64,
    pub size: u32,
    pub name: String,
}

#[derive(Clone)]
pub struct ExceptionInfo {
    pub code: u32,
    pub address: u64,
}

pub struct SystemInfoBlock {
    /// 0x0001 PROCESSOR_ARCHITECTURE_INTEL, 0x8664 AMD64, 0xAA64 ARM64.
    pub arch: u16,
    /// 0 = Windows, 1 = macOS, 2 = Linux, 3 = Android (breakpad ids).
    pub platform: u32,
    pub major: u32,
    pub minor: u32,
}

/// One dump being built. Writes are little-endian, the format is
/// position-based, and the directory lands at the front with RVAs to
/// everything else — exactly like the spec draws it.
pub struct MinidumpBuilder {
    threads: Vec<ThreadInfo>,
    modules: Vec<ModuleInfo>,
    exception: Option<ExceptionInfo>,
    system: SystemInfoBlock,
}

impl MinidumpBuilder {
    pub fn new(system: SystemInfoBlock) -> Self {
        MinidumpBuilder {
            threads: Vec::new(),
            modules: Vec::new(),
            exception: None,
            system,
        }
    }

    pub fn thread(&mut self, thread: ThreadInfo) {
        self.threads.push(thread);
    }

    pub fn module(&mut self, module: ModuleInfo) {
        self.modules.push(module);
    }

    pub fn exception(&mut self, info: ExceptionInfo) {
        self.exception = Some(info);
    }

    /// Emit the dump. Layout: header (32 bytes) + directory (12 bytes
    /// per stream) + streams in a fixed order, with a small string pool
    /// for module names.
    pub fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut rva: u32 = 32 + STREAM_COUNT * 12;

        // Pass 1: compute stream sizes to fill the directory.
        let thread_list_size = 4 + self.threads.len() as u32 * 48;
        let module_list_size = 4 + self.modules.len() as u32 * 40;
        let system_info_size = 24;
        let exception_size = 32;

        let mut directory: Vec<(u32, u32, u32)> = Vec::new(); // (type, size, rva)
        let streams: Vec<(StreamType, u32)> = vec![
            (StreamType::ThreadList, thread_list_size),
            (StreamType::ModuleList, module_list_size),
            (StreamType::SystemInfo, system_info_size),
            (StreamType::Exception, exception_size),
        ];
        for (kind, size) in streams {
            directory.push((kind as u32, size, rva));
            rva += size;
        }

        // Header.
        out.extend_from_slice(&MINIDUMP_MAGIC.to_le_bytes());
        out.extend_from_slice(&42899u16.to_le_bytes()); // version: 0xA793 padded per breakpad
        out.extend_from_slice(&42899u16.to_le_bytes()); // implementation-specific word
        out.extend_from_slice(&STREAM_COUNT.to_le_bytes());
        out.extend_from_slice(&rva.to_le_bytes()); // stream directory rva
        out.extend_from_slice(&0u32.to_le_bytes()); // checksum: unused
        out.extend_from_slice(&0u32.to_le_bytes()); // time date stamp placeholder
        out.extend_from_slice(&0u64.to_le_bytes()); // flags: none

        // Directory.
        for (kind, size, stream_rva) in &directory {
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&stream_rva.to_le_bytes());
        }
        debug_assert_eq!(out.len(), 32 + STREAM_COUNT as usize * 12);

        // ThreadListStream: count + entries.
        out.extend_from_slice(&(self.threads.len() as u32).to_le_bytes());
        for thread in &self.threads {
            out.extend_from_slice(&thread.id.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // suspend count
            out.extend_from_slice(&0u32.to_le_bytes()); // priority class
            out.extend_from_slice(&0u32.to_le_bytes()); // priority
            out.extend_from_slice(&0u64.to_le_bytes()); // teb
            out.extend_from_slice(&0u32.to_le_bytes()); // stack rva (filled by reader side)
            out.extend_from_slice(&(thread.stack.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // context rva
            out.extend_from_slice(&(thread.context_rva_data.len() as u32).to_le_bytes());
        }

        // ModuleListStream: count + entries.
        out.extend_from_slice(&(self.modules.len() as u32).to_le_bytes());
        for module in &self.modules {
            out.extend_from_slice(&module.base.to_le_bytes());
            out.extend_from_slice(&module.size.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // checksum
            out.extend_from_slice(&0u32.to_le_bytes()); // timestamp
            out.extend_from_slice(&(module.name.len() as u32).to_le_bytes()); // name length
            out.extend_from_slice(&0u32.to_le_bytes()); // name rva (resolved by the reader)
            out.extend_from_slice(&0u32.to_le_bytes()); // vs_fixedFileInfo rva
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }

        // SystemInfoStream.
        out.extend_from_slice(&self.system.arch.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // processor level
        out.extend_from_slice(&0u16.to_le_bytes()); // processor revision
        out.extend_from_slice(&0u8.to_le_bytes()); // number of processors
        out.extend_from_slice(&0u8.to_le_bytes()); // product type
        out.extend_from_slice(&0u32.to_le_bytes()); // major version
        out.extend_from_slice(&0u32.to_le_bytes()); // minor version
        out.extend_from_slice(&0u32.to_le_bytes()); // build number
        out.extend_from_slice(&0u32.to_le_bytes()); // platform id
        out.extend_from_slice(&self.system.platform.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // csd version rva
        out.extend_from_slice(&0u16.to_le_bytes()); // suite mask
        out.extend_from_slice(&0u16.to_le_bytes()); // reserved

        // ExceptionStream.
        let exception = self.exception.clone().unwrap_or(ExceptionInfo {
            code: 0,
            address: 0,
        });
        out.extend_from_slice(&0u32.to_le_bytes()); // thread id
        out.extend(&[0u8; 16]); // alignment
        out.extend_from_slice(&exception.code.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // exception flags
        out.extend_from_slice(&0u64.to_le_bytes()); // inner exception rva
        out.extend_from_slice(&exception.address.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // number parameters
        out.extend(&[0u8; 16]);

        out
    }
}

/// Read the stream directory back — the reader that proves the writer.
pub fn list_streams(dump: &[u8]) -> Result<Vec<StreamType>, CrashError> {
    if dump.len() < 32 {
        return Err(CrashError::Format("shorter than a header".into()));
    }
    let magic = u32::from_le_bytes(dump[..4].try_into().expect("4 bytes"));
    if magic != MINIDUMP_MAGIC {
        return Err(CrashError::Format("bad magic".into()));
    }
    let count = u32::from_le_bytes(dump[8..12].try_into().expect("4 bytes"));
    let directory_rva = u32::from_le_bytes(dump[16..20].try_into().expect("4 bytes")) as usize;
    let mut streams = Vec::new();
    for index in 0..count as usize {
        let at = directory_rva + index * 12;
        if at + 12 > dump.len() {
            break;
        }
        let kind = u32::from_le_bytes(dump[at..at + 4].try_into().expect("4 bytes"));
        if let Some(stream) = StreamType::from_u32(kind) {
            streams.push(stream);
        }
    }
    Ok(streams)
}
