//! Direct access to Linux's versioned `clock_gettime` vDSO export.
//!
//! libc normally reaches this function too, but the dynamic-linker and wrapper
//! path is not free on every libc/architecture pair. Wall-clock selectors use
//! this module as an independent candidate and retain it only when the complete
//! public path wins the same runtime tournament as every other provider.
//!
//! The resolver asks only for the clock symbol it will call. In particular it
//! does not make unrelated vDSO exports mandatory, so an older or vendor kernel
//! can omit (for example) `getrandom` without disabling this route. The ELF
//! parser follows Linux's CC0 reference parser while bounding every table walk
//! to the mapped `PT_LOAD` image.

use core::mem::{MaybeUninit, size_of};
use core::sync::atomic::{AtomicUsize, Ordering};

const EI_NIDENT: usize = 16;
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
#[cfg(target_pointer_width = "32")]
const ELFCLASS32: u8 = 1;
#[cfg(target_pointer_width = "64")]
const ELFCLASS64: u8 = 2;
#[cfg(target_endian = "little")]
const ELFDATA2LSB: u8 = 1;
#[cfg(target_endian = "big")]
const ELFDATA2MSB: u8 = 2;
const EV_CURRENT: u8 = 1;
const ET_DYN: u16 = 3;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PF_R: u32 = 4;
const PF_X: u32 = 1;

const DT_NULL: i64 = 0;
const DT_HASH: i64 = 4;
const DT_STRTAB: i64 = 5;
const DT_SYMTAB: i64 = 6;
const DT_STRSZ: i64 = 10;
const DT_SYMENT: i64 = 11;
const DT_GNU_HASH: i64 = 0x6fff_fef5;
const DT_VERSYM: i64 = 0x6fff_fff0;
const DT_VERDEF: i64 = 0x6fff_fffc;
const DT_VERDEFNUM: i64 = 0x6fff_fffd;

const SHN_UNDEF: u16 = 0;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;
const STT_NOTYPE: u8 = 0;
const STT_FUNC: u8 = 2;
const VER_DEF_CURRENT: u16 = 1;
const VER_FLG_BASE: u16 = 1;

const MAX_PROGRAM_HEADERS: usize = 64;
const MAX_LOAD_RANGES: usize = MAX_PROGRAM_HEADERS;
const MAX_DYNAMIC_ENTRIES: usize = 1024;
const MAX_VERSION_DEFINITIONS: usize = 128;
const MAX_HASH_CHAIN: usize = 65_536;
const HEADER_PAGE_FLOOR: usize = 4096;

static CLOCK_GETTIME: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_pointer_width = "32")]
static CLOCK_GETTIME64: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "32")]
struct Elf32Header {
  ident: [u8; EI_NIDENT],
  kind: u16,
  machine: u16,
  version: u32,
  entry: u32,
  program_offset: u32,
  section_offset: u32,
  flags: u32,
  header_size: u16,
  program_entry_size: u16,
  program_count: u16,
  section_entry_size: u16,
  section_count: u16,
  section_names: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "64")]
struct Elf64Header {
  ident: [u8; EI_NIDENT],
  kind: u16,
  machine: u16,
  version: u32,
  entry: u64,
  program_offset: u64,
  section_offset: u64,
  flags: u32,
  header_size: u16,
  program_entry_size: u16,
  program_count: u16,
  section_entry_size: u16,
  section_count: u16,
  section_names: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "32")]
struct Elf32ProgramHeader {
  kind: u32,
  offset: u32,
  virtual_address: u32,
  physical_address: u32,
  file_size: u32,
  memory_size: u32,
  flags: u32,
  align: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "64")]
struct Elf64ProgramHeader {
  kind: u32,
  flags: u32,
  offset: u64,
  virtual_address: u64,
  physical_address: u64,
  file_size: u64,
  memory_size: u64,
  align: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "32")]
struct Elf32Dynamic {
  tag: i32,
  value: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "64")]
struct Elf64Dynamic {
  tag: i64,
  value: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "32")]
struct Elf32Symbol {
  name: u32,
  value: u32,
  size: u32,
  info: u8,
  other: u8,
  section: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_pointer_width = "64")]
struct Elf64Symbol {
  name: u32,
  info: u8,
  other: u8,
  section: u16,
  value: u64,
  size: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VersionDefinition {
  version: u16,
  flags: u16,
  index: u16,
  count: u16,
  hash: u32,
  auxiliary: u32,
  next: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct VersionAuxiliary {
  name: u32,
  next: u32,
}

#[cfg(target_pointer_width = "32")]
type NativeHeader = Elf32Header;
#[cfg(target_pointer_width = "64")]
type NativeHeader = Elf64Header;
#[cfg(target_pointer_width = "32")]
type NativeProgramHeader = Elf32ProgramHeader;
#[cfg(target_pointer_width = "64")]
type NativeProgramHeader = Elf64ProgramHeader;
#[cfg(target_pointer_width = "32")]
type NativeDynamic = Elf32Dynamic;
#[cfg(target_pointer_width = "64")]
type NativeDynamic = Elf64Dynamic;
#[cfg(target_pointer_width = "32")]
type NativeSymbol = Elf32Symbol;
#[cfg(target_pointer_width = "64")]
type NativeSymbol = Elf64Symbol;

#[derive(Clone, Copy)]
struct ProgramHeader {
  kind: u32,
  flags: u32,
  offset: usize,
  virtual_address: usize,
  file_size: usize,
  memory_size: usize,
}

#[derive(Clone, Copy)]
struct Symbol {
  name: usize,
  value: usize,
  info: u8,
  section: u16,
}

#[derive(Clone, Copy)]
struct AddressRange {
  start: usize,
  end: usize,
}

impl AddressRange {
  const EMPTY: Self = Self { start: 0, end: 0 };

  fn new(start: usize, size: usize) -> Option<Self> {
    let end = start.checked_add(size)?;
    (end >= start).then_some(Self { start, end })
  }

  fn contains(self, address: usize, size: usize) -> bool {
    address >= self.start && address.checked_add(size).is_some_and(|end| end <= self.end)
  }
}

#[derive(Clone, Copy)]
struct AddressRanges {
  ranges: [AddressRange; MAX_LOAD_RANGES],
  count: usize,
}

impl AddressRanges {
  const fn new() -> Self {
    Self { ranges: [AddressRange::EMPTY; MAX_LOAD_RANGES], count: 0 }
  }

  fn insert(&mut self, mut range: AddressRange) -> Option<()> {
    if range.start == range.end {
      return Some(());
    }

    let mut index = 0;
    while index < self.count {
      let existing = self.ranges[index];
      if range.start <= existing.end && existing.start <= range.end {
        range.start = range.start.min(existing.start);
        range.end = range.end.max(existing.end);
        self.count -= 1;
        self.ranges[index] = self.ranges[self.count];
        index = 0;
      } else {
        index += 1;
      }
    }

    if self.count == MAX_LOAD_RANGES {
      return None;
    }
    self.ranges[self.count] = range;
    self.count += 1;
    Some(())
  }

  fn contains(&self, address: usize, size: usize) -> bool {
    self.ranges[..self.count].iter().any(|range| range.contains(address, size))
  }
}

struct Image {
  load_bias: usize,
  mapped: AddressRanges,
  executable: AddressRanges,
  symbols: usize,
  strings: usize,
  strings_size: usize,
  sysv_hash: Option<usize>,
  gnu_hash: Option<usize>,
  versions: usize,
  definitions: usize,
  definition_count: usize,
}

impl Image {
  /// Parse the kernel mapping named by `AT_SYSINFO_EHDR`.
  ///
  /// # Safety
  ///
  /// `base` must be the value supplied by the kernel for this process. The
  /// parser validates all derived addresses before dereferencing them.
  unsafe fn parse(base: usize) -> Option<Self> {
    if base == 0 || base & (core::mem::align_of::<NativeHeader>() - 1) != 0 {
      return None;
    }
    // SAFETY: AT_SYSINFO_EHDR points at at least the ELF header page.
    let header = unsafe { core::ptr::read_unaligned(base as *const NativeHeader) };
    if header_ident(&header)[..4] != ELF_MAGIC
      || header_ident(&header)[4] != native_elf_class()
      || header_ident(&header)[5] != native_elf_data()
      || header_ident(&header)[6] != EV_CURRENT
      || header_kind(&header) != ET_DYN
      || header_version(&header) != u32::from(EV_CURRENT)
      || usize::from(header_size(&header)) != size_of::<NativeHeader>()
      || usize::from(header_program_entry_size(&header)) != size_of::<NativeProgramHeader>()
    {
      return None;
    }

    let program_count = usize::from(header_program_count(&header));
    if program_count == 0 || program_count > MAX_PROGRAM_HEADERS {
      return None;
    }
    let program_bytes = program_count.checked_mul(size_of::<NativeProgramHeader>())?;
    let program_offset = header_program_offset(&header)?;
    let program_end = program_offset.checked_add(program_bytes)?;
    if program_offset < size_of::<NativeHeader>() || program_end > HEADER_PAGE_FLOOR {
      return None;
    }

    let read_program = |index: usize| -> Option<ProgramHeader> {
      let address = base
        .checked_add(program_offset)?
        .checked_add(index.checked_mul(size_of::<NativeProgramHeader>())?)?;
      // SAFETY: the bounded program-header table is in the ELF header page.
      let native = unsafe { core::ptr::read_unaligned(address as *const NativeProgramHeader) };
      program_header(native)
    };

    // AT_SYSINFO_EHDR names file offset zero. Establish the runtime load bias
    // from the readable PT_LOAD that actually contains both the ELF header and
    // the complete program-header table. Other LOAD segments may have virtual
    // gaps or a different p_vaddr - p_offset relationship.
    let mut load_bias = None;
    for index in 0..program_count {
      let program = read_program(index)?;
      match program.kind {
        PT_LOAD => {
          if program.file_size > program.memory_size {
            return None;
          }
          let _ = program.offset.checked_add(program.file_size)?;
          let _ = program.virtual_address.checked_add(program.memory_size)?;
          if program.memory_size == 0 {
            continue;
          }
          if program.offset == 0
            && program.file_size >= program_end
            && program.memory_size >= program_end
            && program.flags & PF_R != 0
          {
            let bias = base.checked_sub(program.virtual_address)?;
            if load_bias.is_some_and(|expected| expected != bias) {
              return None;
            }
            load_bias = Some(bias);
          }
        }
        PT_DYNAMIC => {
          if program.file_size > program.memory_size {
            return None;
          }
          let _ = program.offset.checked_add(program.file_size)?;
          let _ = program.virtual_address.checked_add(program.memory_size)?;
        }
        PT_INTERP => return None,
        _ => {}
      }
    }

    let load_bias = load_bias?;
    let mut mapped = AddressRanges::new();
    let mut executable = AddressRanges::new();
    let mut dynamic = None;
    for index in 0..program_count {
      let program = read_program(index)?;
      match program.kind {
        PT_LOAD if program.memory_size != 0 => {
          let start = load_bias.checked_add(program.virtual_address)?;
          let range = AddressRange::new(start, program.memory_size)?;
          if program.flags & PF_R != 0 {
            mapped.insert(range)?;
          }
          if program.flags & (PF_R | PF_X) == (PF_R | PF_X) {
            executable.insert(range)?;
          }
        }
        PT_DYNAMIC => {
          if dynamic.is_some() || program.file_size == 0 {
            return None;
          }
          let address = load_bias.checked_add(program.virtual_address)?;
          dynamic = Some((address, program.file_size));
        }
        // A vDSO must not request an interpreter. Other program headers such
        // as PT_NOTE, PT_GNU_EH_FRAME, and PT_GNU_RELRO are harmless here;
        // only addresses derived from validated LOAD/DYNAMIC ranges are read.
        PT_INTERP => return None,
        _ => {}
      }
    }

    if !mapped.contains(base, program_end) || executable.count == 0 {
      return None;
    }
    let (dynamic, dynamic_size) = dynamic?;
    if !mapped.contains(dynamic, dynamic_size) || dynamic_size % size_of::<NativeDynamic>() != 0 {
      return None;
    }
    let dynamic_count = dynamic_size / size_of::<NativeDynamic>();
    if dynamic_count == 0 || dynamic_count > MAX_DYNAMIC_ENTRIES {
      return None;
    }

    let mut symbols = None;
    let mut strings = None;
    let mut strings_size = None;
    let mut sysv_hash = None;
    let mut gnu_hash = None;
    let mut versions = None;
    let mut definitions = None;
    let mut definition_count = None;
    let mut terminated = false;
    for index in 0..dynamic_count {
      let address = dynamic.checked_add(index.checked_mul(size_of::<NativeDynamic>())?)?;
      // SAFETY: the dynamic segment and entry count were bounded above.
      let entry = unsafe { core::ptr::read_unaligned(address as *const NativeDynamic) };
      let (tag, value) = dynamic_entry(entry)?;
      if tag == DT_NULL {
        terminated = true;
        break;
      }
      let translated =
        || load_bias.checked_add(value).filter(|address| mapped.contains(*address, 1));
      match tag {
        DT_STRTAB => strings = translated(),
        DT_SYMTAB => symbols = translated(),
        DT_STRSZ => strings_size = Some(value),
        DT_SYMENT if value != size_of::<NativeSymbol>() => return None,
        DT_HASH => sysv_hash = translated(),
        DT_GNU_HASH => gnu_hash = translated(),
        DT_VERSYM => versions = translated(),
        DT_VERDEF => definitions = translated(),
        DT_VERDEFNUM => definition_count = Some(value),
        _ => {}
      }
    }
    if !terminated {
      return None;
    }

    let image = Self {
      load_bias,
      mapped,
      executable,
      symbols: symbols?,
      strings: strings?,
      strings_size: strings_size?,
      sysv_hash,
      gnu_hash,
      versions: versions?,
      definitions: definitions?,
      definition_count: definition_count?
        .checked_sub(1)
        .filter(|count| *count < MAX_VERSION_DEFINITIONS)?
        + 1,
    };
    if image.strings_size == 0
      || !image.mapped.contains(image.strings, image.strings_size)
      || (!image.mapped.contains(image.symbols, size_of::<NativeSymbol>()))
      || image.definition_count == 0
      || (image.sysv_hash.is_none() && image.gnu_hash.is_none())
    {
      return None;
    }
    Some(image)
  }

  unsafe fn symbol(&self, version: &[u8], name: &[u8]) -> Option<usize> {
    if let Some(hash) = self.gnu_hash {
      // SAFETY: each lookup helper bounds all derived table accesses.
      if let Some(symbol) = unsafe { self.gnu_symbol(hash, version, name) } {
        return Some(symbol);
      }
    }
    if let Some(hash) = self.sysv_hash {
      // SAFETY: each lookup helper bounds all derived table accesses.
      return unsafe { self.sysv_symbol(hash, version, name) };
    }
    None
  }

  unsafe fn gnu_symbol(&self, table: usize, version: &[u8], name: &[u8]) -> Option<usize> {
    let buckets = self.read_u32(table)? as usize;
    let symbol_offset = self.read_u32(table.checked_add(4)?)? as usize;
    let bloom_words = self.read_u32(table.checked_add(8)?)? as usize;
    if buckets == 0 || bloom_words > MAX_HASH_CHAIN {
      return None;
    }
    let bloom_bytes = bloom_words.checked_mul(size_of::<usize>())?;
    let bucket_table = table.checked_add(16)?.checked_add(bloom_bytes)?;
    if !self.mapped.contains(bucket_table, buckets.checked_mul(4)?) {
      return None;
    }
    let hash = gnu_hash(name);
    let mut index =
      self.read_u32(bucket_table.checked_add((hash as usize % buckets) * 4)?)? as usize;
    if index == 0 || index < symbol_offset {
      return None;
    }
    let chain = bucket_table
      .checked_add(buckets.checked_mul(4)?)?
      .checked_add((index - symbol_offset).checked_mul(4)?)?;
    for offset in 0..MAX_HASH_CHAIN {
      let value = self.read_u32(chain.checked_add(offset.checked_mul(4)?)?)?;
      if (hash | 1) == (value | 1) {
        // SAFETY: check_symbol bounds the indexed symbol and all strings.
        if let Some(address) = unsafe { self.check_symbol(index, version, name) } {
          return Some(address);
        }
      }
      if value & 1 != 0 {
        return None;
      }
      index = index.checked_add(1)?;
    }
    None
  }

  unsafe fn sysv_symbol(&self, table: usize, version: &[u8], name: &[u8]) -> Option<usize> {
    let entry_size = sysv_hash_entry_size();
    let buckets = self.read_hash_entry(table)?;
    let chains = self.read_hash_entry(table.checked_add(entry_size)?)?;
    if buckets == 0 || chains == 0 || buckets > MAX_HASH_CHAIN || chains > MAX_HASH_CHAIN {
      return None;
    }
    let bucket_table = table.checked_add(entry_size.checked_mul(2)?)?;
    let chain_table = bucket_table.checked_add(buckets.checked_mul(entry_size)?)?;
    if !self.mapped.contains(bucket_table, buckets.checked_mul(entry_size)?)
      || !self.mapped.contains(chain_table, chains.checked_mul(entry_size)?)
    {
      return None;
    }
    let bucket = (elf_hash(name) as usize) % buckets;
    let mut index = self.read_hash_entry(bucket_table.checked_add(bucket * entry_size)?)?;
    for _ in 0..chains.min(MAX_HASH_CHAIN) {
      if index == 0 || index >= chains {
        return None;
      }
      // SAFETY: check_symbol bounds the indexed symbol and all strings.
      if let Some(address) = unsafe { self.check_symbol(index, version, name) } {
        return Some(address);
      }
      index = self.read_hash_entry(chain_table.checked_add(index.checked_mul(entry_size)?)?)?;
    }
    None
  }

  unsafe fn check_symbol(&self, index: usize, version: &[u8], name: &[u8]) -> Option<usize> {
    let address = self.symbols.checked_add(index.checked_mul(size_of::<NativeSymbol>())?)?;
    if !self.mapped.contains(address, size_of::<NativeSymbol>()) {
      return None;
    }
    // SAFETY: the native symbol lies in the mapped image.
    let native = unsafe { core::ptr::read_unaligned(address as *const NativeSymbol) };
    let symbol = symbol(native)?;
    let kind = symbol.info & 0x0f;
    let binding = symbol.info >> 4;
    if symbol.section == SHN_UNDEF
      || !matches!(kind, STT_FUNC | STT_NOTYPE)
      || !matches!(binding, STB_GLOBAL | STB_WEAK)
      || !self.string_equals(symbol.name, name)
    {
      return None;
    }
    let version_address = self.versions.checked_add(index.checked_mul(2)?)?;
    let version_index = self.read_u16(version_address)? & 0x7fff;
    // SAFETY: version_matches bounds every version-definition and string-table
    // access to this image's validated PT_LOAD range.
    if !unsafe { self.version_matches(version_index, version) } {
      return None;
    }
    let function = self.load_bias.checked_add(symbol.value)?;
    self.executable.contains(function, 1).then_some(function)
  }

  unsafe fn version_matches(&self, index: u16, expected: &[u8]) -> bool {
    let expected_hash = elf_hash(expected);
    let mut address = self.definitions;
    for _ in 0..self.definition_count {
      if !self.mapped.contains(address, size_of::<VersionDefinition>()) {
        return false;
      }
      // SAFETY: the version definition is wholly inside the image.
      let definition = unsafe { core::ptr::read_unaligned(address as *const VersionDefinition) };
      if definition.version != VER_DEF_CURRENT {
        return false;
      }
      if definition.flags & VER_FLG_BASE == 0 && definition.index & 0x7fff == index {
        let Some(auxiliary_address) = address.checked_add(definition.auxiliary as usize) else {
          return false;
        };
        if !self.mapped.contains(auxiliary_address, size_of::<VersionAuxiliary>()) {
          return false;
        }
        // SAFETY: the auxiliary record is wholly inside the image.
        let auxiliary =
          unsafe { core::ptr::read_unaligned(auxiliary_address as *const VersionAuxiliary) };
        return definition.hash == expected_hash
          && self.string_equals(auxiliary.name as usize, expected);
      }
      if definition.next == 0 {
        return false;
      }
      let Some(next) = address.checked_add(definition.next as usize) else {
        return false;
      };
      if next <= address {
        return false;
      }
      address = next;
    }
    false
  }

  fn string_equals(&self, offset: usize, expected: &[u8]) -> bool {
    if offset >= self.strings_size || expected.len() >= self.strings_size - offset {
      return false;
    }
    let Some(address) = self.strings.checked_add(offset) else {
      return false;
    };
    if !self.mapped.contains(address, expected.len() + 1) {
      return false;
    }
    // SAFETY: the bounded string bytes lie inside the mapped string table.
    unsafe {
      core::slice::from_raw_parts(address as *const u8, expected.len()) == expected
        && *(address as *const u8).add(expected.len()) == 0
    }
  }

  fn read_u16(&self, address: usize) -> Option<u16> {
    self.mapped.contains(address, 2).then(|| {
      // SAFETY: the two bytes lie inside the mapped image.
      unsafe { core::ptr::read_unaligned(address as *const u16) }
    })
  }

  fn read_u32(&self, address: usize) -> Option<u32> {
    self.mapped.contains(address, 4).then(|| {
      // SAFETY: the four bytes lie inside the mapped image.
      unsafe { core::ptr::read_unaligned(address as *const u32) }
    })
  }

  fn read_hash_entry(&self, address: usize) -> Option<usize> {
    #[cfg(target_arch = "s390x")]
    {
      self.mapped.contains(address, 8).then(|| {
        // SAFETY: the eight bytes lie inside the mapped image.
        unsafe { core::ptr::read_unaligned(address as *const u64) as usize }
      })
    }
    #[cfg(not(target_arch = "s390x"))]
    {
      self.read_u32(address).map(|value| value as usize)
    }
  }
}

#[cfg(target_pointer_width = "32")]
#[repr(C)]
struct KernelTimespec64 {
  seconds: i64,
  nanos: i64,
}

/// Resolve and install the platform's versioned clock symbols.
///
/// This function deliberately performs no process-global locking. Each wall
/// selector can parse the immutable kernel image independently, which keeps a
/// same-thread signal reentry from waiting on resolver state. Concurrent
/// successful parses publish the same kernel address.
pub(crate) fn install() -> bool {
  // SAFETY: getauxval has no pointer arguments and AT_SYSINFO_EHDR is immutable
  // process startup metadata supplied by the kernel.
  let base = unsafe { libc::getauxval(libc::AT_SYSINFO_EHDR) } as usize;
  // SAFETY: base came directly from AT_SYSINFO_EHDR above.
  let Some(image) = (unsafe { Image::parse(base) }) else {
    return false;
  };
  // SAFETY: symbol() bounds every ELF lookup to image's PT_LOAD range.
  let native = unsafe { image.symbol(platform_version(), platform_symbol()) };
  if let Some(function) = native {
    CLOCK_GETTIME.store(function, Ordering::Release);
  }
  #[cfg(target_pointer_width = "32")]
  {
    // The time64 export is independent: old kernels may expose only time32,
    // while modern kernels can give the two symbols different entry paths.
    // SAFETY: the same bounded lookup is used for the versioned time64 symbol.
    let time64 = unsafe { image.symbol(platform_version(), b"__vdso_clock_gettime64") };
    if let Some(function) = time64 {
      CLOCK_GETTIME64.store(function, Ordering::Release);
    }
    native.is_some() || time64.is_some()
  }
  #[cfg(target_pointer_width = "64")]
  {
    native.is_some()
  }
}

/// Whether Arm Linux left its versioned direct clock symbol installed.
///
/// The 32-bit Arm kernel null-patches this symbol when its boot-time
/// `cntvct_functional()` check finds no architectural timer or firmware did
/// not configure the virtual counter for user access. A successfully resolved
/// symbol is therefore the kernel-provided non-faulting gate for direct CNTVCT.
#[cfg(target_arch = "arm")]
#[inline]
pub(crate) fn arm_cntvct_access_proven() -> bool {
  CLOCK_GETTIME.load(Ordering::Acquire) != 0
}

/// Read a nanosecond CLOCK_MONOTONIC-family value through the installed vDSO.
#[inline(always)]
pub(crate) fn clock_nanos(clock_id: libc::clockid_t) -> Option<u64> {
  let function = CLOCK_GETTIME.load(Ordering::Relaxed);
  if function == 0 {
    return None;
  }
  let mut value = MaybeUninit::<libc::timespec>::uninit();
  // SAFETY: install accepted a versioned clock_gettime export with this
  // architecture's documented ABI, and value is writable output storage.
  let status = unsafe { call_clock_gettime(function, clock_id, value.as_mut_ptr().cast()) };
  if status != 0 {
    return None;
  }
  // SAFETY: successful clock_gettime initialized both fields.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.tv_sec).ok()?;
  let nanos = u32::try_from(value.tv_nsec).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

/// Read through the independent Y2038-safe vDSO export on 32-bit kernels.
#[cfg(target_pointer_width = "32")]
#[inline(always)]
pub(crate) fn clock_nanos_time64(clock_id: libc::clockid_t) -> Option<u64> {
  let function = CLOCK_GETTIME64.load(Ordering::Relaxed);
  if function == 0 {
    return None;
  }
  let mut value = MaybeUninit::<KernelTimespec64>::uninit();
  // SAFETY: install resolved the versioned time64 entry and value has the
  // kernel_timespec layout required by that ABI.
  let status = unsafe { call_clock_gettime(function, clock_id, value.as_mut_ptr().cast()) };
  if status != 0 {
    return None;
  }
  // SAFETY: successful clock_gettime64 initialized both fields.
  let value = unsafe { value.assume_init() };
  let seconds = u64::try_from(value.seconds).ok()?;
  let nanos = u32::try_from(value.nanos).ok()?;
  if nanos >= 1_000_000_000 {
    return None;
  }
  seconds.checked_mul(1_000_000_000)?.checked_add(u64::from(nanos))
}

#[cfg(not(target_arch = "powerpc64"))]
unsafe fn call_clock_gettime(
  function: usize,
  clock_id: libc::clockid_t,
  value: *mut core::ffi::c_void,
) -> libc::c_int {
  type ClockGettime = unsafe extern "C" fn(libc::clockid_t, *mut core::ffi::c_void) -> libc::c_int;
  // SAFETY: install resolved this address from the executable PT_LOAD segment
  // as the architecture's versioned clock_gettime function.
  let function = unsafe { core::mem::transmute::<usize, ClockGettime>(function) };
  // SAFETY: upheld by this function's caller.
  unsafe { function(clock_id, value) }
}

#[cfg(target_arch = "powerpc64")]
unsafe fn call_clock_gettime(
  function: usize,
  clock_id: libc::clockid_t,
  value: *mut core::ffi::c_void,
) -> libc::c_int {
  let status: libc::c_long;
  // PowerPC vDSO entry points use the syscall calling convention rather than
  // the ELF function-descriptor convention. Branch to the symbol's code
  // address through CTR and declare the complete volatile register set.
  // SAFETY: function is an executable vDSO symbol and value is writable.
  unsafe {
    core::arch::asm!(
      "mtctr 0",
      "bctrl",
      inlateout("r0") function => _,
      inlateout("r3") libc::c_long::from(clock_id) => status,
      inlateout("r4") value => _,
      lateout("r5") _,
      lateout("r6") _,
      lateout("r7") _,
      lateout("r8") _,
      lateout("r9") _,
      lateout("r10") _,
      lateout("r11") _,
      lateout("r12") _,
      lateout("cr0") _,
      lateout("cr1") _,
      lateout("cr5") _,
      lateout("cr6") _,
      lateout("cr7") _,
      lateout("xer") _,
      lateout("lr") _,
      lateout("ctr") _,
    );
  }
  status as libc::c_int
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
const fn platform_version() -> &'static [u8] {
  b"LINUX_2.6"
}
#[cfg(target_arch = "arm")]
const fn platform_version() -> &'static [u8] {
  b"LINUX_2.6"
}
#[cfg(target_arch = "aarch64")]
const fn platform_version() -> &'static [u8] {
  b"LINUX_2.6.39"
}
#[cfg(target_arch = "riscv64")]
const fn platform_version() -> &'static [u8] {
  b"LINUX_4.15"
}
#[cfg(target_arch = "loongarch64")]
const fn platform_version() -> &'static [u8] {
  b"LINUX_5.10"
}
#[cfg(target_arch = "powerpc64")]
const fn platform_version() -> &'static [u8] {
  b"LINUX_2.6.15"
}
#[cfg(target_arch = "s390x")]
const fn platform_version() -> &'static [u8] {
  b"LINUX_2.6.29"
}

#[cfg(any(
  target_arch = "x86",
  target_arch = "x86_64",
  target_arch = "arm",
  target_arch = "riscv64",
  target_arch = "loongarch64"
))]
const fn platform_symbol() -> &'static [u8] {
  b"__vdso_clock_gettime"
}
#[cfg(any(target_arch = "aarch64", target_arch = "powerpc64", target_arch = "s390x"))]
const fn platform_symbol() -> &'static [u8] {
  b"__kernel_clock_gettime"
}

#[cfg(target_pointer_width = "32")]
const fn native_elf_class() -> u8 {
  ELFCLASS32
}
#[cfg(target_pointer_width = "64")]
const fn native_elf_class() -> u8 {
  ELFCLASS64
}
#[cfg(target_endian = "little")]
const fn native_elf_data() -> u8 {
  ELFDATA2LSB
}
#[cfg(target_endian = "big")]
const fn native_elf_data() -> u8 {
  ELFDATA2MSB
}

#[cfg(target_pointer_width = "32")]
fn header_ident(header: &NativeHeader) -> &[u8; EI_NIDENT] {
  &header.ident
}
#[cfg(target_pointer_width = "64")]
fn header_ident(header: &NativeHeader) -> &[u8; EI_NIDENT] {
  &header.ident
}
#[cfg(target_pointer_width = "32")]
const fn header_kind(header: &NativeHeader) -> u16 {
  header.kind
}
#[cfg(target_pointer_width = "64")]
const fn header_kind(header: &NativeHeader) -> u16 {
  header.kind
}
#[cfg(target_pointer_width = "32")]
const fn header_version(header: &NativeHeader) -> u32 {
  header.version
}
#[cfg(target_pointer_width = "64")]
const fn header_version(header: &NativeHeader) -> u32 {
  header.version
}
#[cfg(target_pointer_width = "32")]
const fn header_size(header: &NativeHeader) -> u16 {
  header.header_size
}
#[cfg(target_pointer_width = "64")]
const fn header_size(header: &NativeHeader) -> u16 {
  header.header_size
}
#[cfg(target_pointer_width = "32")]
const fn header_program_entry_size(header: &NativeHeader) -> u16 {
  header.program_entry_size
}
#[cfg(target_pointer_width = "64")]
const fn header_program_entry_size(header: &NativeHeader) -> u16 {
  header.program_entry_size
}
#[cfg(target_pointer_width = "32")]
const fn header_program_count(header: &NativeHeader) -> u16 {
  header.program_count
}
#[cfg(target_pointer_width = "64")]
const fn header_program_count(header: &NativeHeader) -> u16 {
  header.program_count
}
#[cfg(target_pointer_width = "32")]
fn header_program_offset(header: &NativeHeader) -> Option<usize> {
  Some(header.program_offset as usize)
}
#[cfg(target_pointer_width = "64")]
fn header_program_offset(header: &NativeHeader) -> Option<usize> {
  usize::try_from(header.program_offset).ok()
}

#[cfg(target_pointer_width = "32")]
fn program_header(header: NativeProgramHeader) -> Option<ProgramHeader> {
  Some(ProgramHeader {
    kind: header.kind,
    flags: header.flags,
    offset: header.offset as usize,
    virtual_address: header.virtual_address as usize,
    file_size: header.file_size as usize,
    memory_size: header.memory_size as usize,
  })
}
#[cfg(target_pointer_width = "64")]
fn program_header(header: NativeProgramHeader) -> Option<ProgramHeader> {
  Some(ProgramHeader {
    kind: header.kind,
    flags: header.flags,
    offset: usize::try_from(header.offset).ok()?,
    virtual_address: usize::try_from(header.virtual_address).ok()?,
    file_size: usize::try_from(header.file_size).ok()?,
    memory_size: usize::try_from(header.memory_size).ok()?,
  })
}

#[cfg(target_pointer_width = "32")]
fn dynamic_entry(entry: NativeDynamic) -> Option<(i64, usize)> {
  Some((i64::from(entry.tag), entry.value as usize))
}
#[cfg(target_pointer_width = "64")]
fn dynamic_entry(entry: NativeDynamic) -> Option<(i64, usize)> {
  Some((entry.tag, usize::try_from(entry.value).ok()?))
}

#[cfg(target_pointer_width = "32")]
fn symbol(symbol: NativeSymbol) -> Option<Symbol> {
  Some(Symbol {
    name: symbol.name as usize,
    value: symbol.value as usize,
    info: symbol.info,
    section: symbol.section,
  })
}
#[cfg(target_pointer_width = "64")]
fn symbol(symbol: NativeSymbol) -> Option<Symbol> {
  Some(Symbol {
    name: symbol.name as usize,
    value: usize::try_from(symbol.value).ok()?,
    info: symbol.info,
    section: symbol.section,
  })
}

#[cfg(target_arch = "s390x")]
const fn sysv_hash_entry_size() -> usize {
  8
}
#[cfg(not(target_arch = "s390x"))]
const fn sysv_hash_entry_size() -> usize {
  4
}

fn elf_hash(name: &[u8]) -> u32 {
  let mut hash = 0_u32;
  for byte in name {
    hash = hash.wrapping_shl(4).wrapping_add(u32::from(*byte));
    let high = hash & 0xf000_0000;
    if high != 0 {
      hash ^= high >> 24;
    }
    hash &= !high;
  }
  hash
}

fn gnu_hash(name: &[u8]) -> u32 {
  let mut hash = 5381_u32;
  for byte in name {
    hash = hash.wrapping_mul(33).wrapping_add(u32::from(*byte));
  }
  hash
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::vec;
  use std::vec::Vec;

  const TEST_IMAGE_SIZE: usize = 0x5000;
  const HEADER_LOAD_SIZE: usize = 0x1000;
  const SECOND_LOAD_OFFSET: usize = 0x1000;
  const SECOND_LOAD_ADDRESS: usize = 0x3000;
  const SECOND_LOAD_SIZE: usize = 0x1000;
  const DYNAMIC_ADDRESS: usize = 0x3100;
  const HASH_ADDRESS: usize = 0x3200;
  const SYMBOL_ADDRESS: usize = 0x3300;
  const STRING_ADDRESS: usize = 0x3400;
  const VERSION_SYMBOL_ADDRESS: usize = 0x3500;
  const VERSION_DEFINITION_ADDRESS: usize = 0x3600;
  const NATIVE_FUNCTION_ADDRESS: usize = 0x3e00;
  const TIME64_FUNCTION_ADDRESS: usize = 0x3e40;
  const PROGRAM_COUNT: usize = 3;
  const DYNAMIC_COUNT: usize = 9;

  #[derive(Clone, Copy)]
  enum TestHash {
    Sysv,
    Gnu,
  }

  struct TestImage {
    storage: Vec<usize>,
  }

  impl TestImage {
    fn new(hash: TestHash) -> Self {
      let words = TEST_IMAGE_SIZE.div_ceil(size_of::<usize>());
      let mut image = Self { storage: vec![0; words] };
      image.write_header(PROGRAM_COUNT);
      image.write_program(0, PT_LOAD, PF_R | PF_X, 0, 0, HEADER_LOAD_SIZE, HEADER_LOAD_SIZE);
      image.write_program(
        1,
        PT_LOAD,
        PF_R | PF_X,
        SECOND_LOAD_OFFSET,
        SECOND_LOAD_ADDRESS,
        SECOND_LOAD_SIZE,
        SECOND_LOAD_SIZE,
      );
      let dynamic_size = DYNAMIC_COUNT * size_of::<NativeDynamic>();
      image.write_program(
        2,
        PT_DYNAMIC,
        PF_R,
        SECOND_LOAD_OFFSET + DYNAMIC_ADDRESS - SECOND_LOAD_ADDRESS,
        DYNAMIC_ADDRESS,
        dynamic_size,
        dynamic_size,
      );

      let mut strings = Vec::new();
      strings.push(0);
      let native_name = strings.len();
      strings.extend_from_slice(platform_symbol());
      strings.push(0);
      let time64_name = strings.len();
      strings.extend_from_slice(b"__vdso_clock_gettime64");
      strings.push(0);
      let version_name = strings.len();
      strings.extend_from_slice(platform_version());
      strings.push(0);
      image.write_bytes(STRING_ADDRESS, &strings);

      image.write_symbol(1, native_name, NATIVE_FUNCTION_ADDRESS, (STB_GLOBAL << 4) | STT_FUNC);
      image.write_symbol(2, time64_name, TIME64_FUNCTION_ADDRESS, (STB_GLOBAL << 4) | STT_FUNC);
      image.write(VERSION_SYMBOL_ADDRESS, 0_u16);
      image.write(VERSION_SYMBOL_ADDRESS + 2, 2_u16);
      image.write(VERSION_SYMBOL_ADDRESS + 4, 2_u16);

      let definition_size = size_of::<VersionDefinition>() + size_of::<VersionAuxiliary>();
      image.write(VERSION_DEFINITION_ADDRESS, VersionDefinition {
        version: VER_DEF_CURRENT,
        flags: VER_FLG_BASE,
        index: 1,
        count: 1,
        hash: 0,
        auxiliary: size_of::<VersionDefinition>() as u32,
        next: definition_size as u32,
      });
      image.write(VERSION_DEFINITION_ADDRESS + size_of::<VersionDefinition>(), VersionAuxiliary {
        name: version_name as u32,
        next: 0,
      });
      image.write(VERSION_DEFINITION_ADDRESS + definition_size, VersionDefinition {
        version: VER_DEF_CURRENT,
        flags: 0,
        index: 2,
        count: 1,
        hash: elf_hash(platform_version()),
        auxiliary: size_of::<VersionDefinition>() as u32,
        next: 0,
      });
      image.write(
        VERSION_DEFINITION_ADDRESS + definition_size + size_of::<VersionDefinition>(),
        VersionAuxiliary { name: version_name as u32, next: 0 },
      );

      match hash {
        TestHash::Sysv => image.write_sysv_hash(),
        TestHash::Gnu => image.write_gnu_hash(),
      }
      let hash_tag = match hash {
        TestHash::Sysv => DT_HASH,
        TestHash::Gnu => DT_GNU_HASH,
      };
      image.write_dynamic(0, DT_STRTAB, STRING_ADDRESS);
      image.write_dynamic(1, DT_SYMTAB, SYMBOL_ADDRESS);
      image.write_dynamic(2, DT_STRSZ, strings.len());
      image.write_dynamic(3, DT_SYMENT, size_of::<NativeSymbol>());
      image.write_dynamic(4, hash_tag, HASH_ADDRESS);
      image.write_dynamic(5, DT_VERSYM, VERSION_SYMBOL_ADDRESS);
      image.write_dynamic(6, DT_VERDEF, VERSION_DEFINITION_ADDRESS);
      image.write_dynamic(7, DT_VERDEFNUM, 2);
      image.write_dynamic(8, DT_NULL, 0);
      image
    }

    fn base(&self) -> usize {
      self.storage.as_ptr() as usize
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
      // SAFETY: the byte slice covers exactly the initialized usize storage.
      unsafe {
        core::slice::from_raw_parts_mut(
          self.storage.as_mut_ptr().cast::<u8>(),
          self.storage.len() * size_of::<usize>(),
        )
      }
    }

    fn write<T: Copy>(&mut self, offset: usize, value: T) {
      assert!(offset.checked_add(size_of::<T>()).is_some_and(|end| end <= TEST_IMAGE_SIZE));
      // SAFETY: the destination is wholly inside the aligned test allocation;
      // unaligned writes permit every native ELF field offset.
      unsafe {
        core::ptr::write_unaligned(self.bytes_mut().as_mut_ptr().add(offset).cast::<T>(), value);
      }
    }

    fn write_bytes(&mut self, offset: usize, value: &[u8]) {
      let end = offset.checked_add(value.len()).expect("test image offset overflow");
      assert!(end <= TEST_IMAGE_SIZE);
      self.bytes_mut()[offset..end].copy_from_slice(value);
    }

    fn write_header(&mut self, program_count: usize) {
      let mut ident = [0_u8; EI_NIDENT];
      ident[..ELF_MAGIC.len()].copy_from_slice(&ELF_MAGIC);
      ident[4] = native_elf_class();
      ident[5] = native_elf_data();
      ident[6] = EV_CURRENT;
      #[cfg(target_pointer_width = "32")]
      let header = Elf32Header {
        ident,
        kind: ET_DYN,
        machine: 0,
        version: u32::from(EV_CURRENT),
        entry: 0,
        program_offset: size_of::<NativeHeader>() as u32,
        section_offset: 0,
        flags: 0,
        header_size: size_of::<NativeHeader>() as u16,
        program_entry_size: size_of::<NativeProgramHeader>() as u16,
        program_count: program_count as u16,
        section_entry_size: 0,
        section_count: 0,
        section_names: 0,
      };
      #[cfg(target_pointer_width = "64")]
      let header = Elf64Header {
        ident,
        kind: ET_DYN,
        machine: 0,
        version: u32::from(EV_CURRENT),
        entry: 0,
        program_offset: size_of::<NativeHeader>() as u64,
        section_offset: 0,
        flags: 0,
        header_size: size_of::<NativeHeader>() as u16,
        program_entry_size: size_of::<NativeProgramHeader>() as u16,
        program_count: program_count as u16,
        section_entry_size: 0,
        section_count: 0,
        section_names: 0,
      };
      self.write(0, header);
    }

    #[allow(clippy::too_many_arguments)]
    fn write_program(
      &mut self,
      index: usize,
      kind: u32,
      flags: u32,
      offset: usize,
      virtual_address: usize,
      file_size: usize,
      memory_size: usize,
    ) {
      #[cfg(target_pointer_width = "32")]
      let program = Elf32ProgramHeader {
        kind,
        offset: offset as u32,
        virtual_address: virtual_address as u32,
        physical_address: 0,
        file_size: file_size as u32,
        memory_size: memory_size as u32,
        flags,
        align: HEADER_PAGE_FLOOR as u32,
      };
      #[cfg(target_pointer_width = "64")]
      let program = Elf64ProgramHeader {
        kind,
        flags,
        offset: offset as u64,
        virtual_address: virtual_address as u64,
        physical_address: 0,
        file_size: file_size as u64,
        memory_size: memory_size as u64,
        align: HEADER_PAGE_FLOOR as u64,
      };
      let program_offset = size_of::<NativeHeader>() + index * size_of::<NativeProgramHeader>();
      self.write(program_offset, program);
    }

    fn write_dynamic(&mut self, index: usize, tag: i64, value: usize) {
      #[cfg(target_pointer_width = "32")]
      let dynamic = Elf32Dynamic { tag: tag as i32, value: value as u32 };
      #[cfg(target_pointer_width = "64")]
      let dynamic = Elf64Dynamic { tag, value: value as u64 };
      self.write(DYNAMIC_ADDRESS + index * size_of::<NativeDynamic>(), dynamic);
    }

    fn write_symbol(&mut self, index: usize, name: usize, value: usize, info: u8) {
      #[cfg(target_pointer_width = "32")]
      let symbol =
        Elf32Symbol { name: name as u32, value: value as u32, size: 1, info, other: 0, section: 1 };
      #[cfg(target_pointer_width = "64")]
      let symbol =
        Elf64Symbol { name: name as u32, info, other: 0, section: 1, value: value as u64, size: 1 };
      self.write(SYMBOL_ADDRESS + index * size_of::<NativeSymbol>(), symbol);
    }

    fn write_hash_entry(&mut self, offset: usize, value: usize) {
      #[cfg(target_arch = "s390x")]
      self.write(offset, value as u64);
      #[cfg(not(target_arch = "s390x"))]
      self.write(offset, value as u32);
    }

    fn write_sysv_hash(&mut self) {
      let entry = sysv_hash_entry_size();
      for (index, value) in [1, 3, 1, 0, 2, 0].into_iter().enumerate() {
        self.write_hash_entry(HASH_ADDRESS + index * entry, value);
      }
    }

    fn write_gnu_hash(&mut self) {
      self.write(HASH_ADDRESS, 1_u32);
      self.write(HASH_ADDRESS + 4, 1_u32);
      self.write(HASH_ADDRESS + 8, 1_u32);
      self.write(HASH_ADDRESS + 12, 0_u32);
      self.write(HASH_ADDRESS + 16, usize::MAX);
      let bucket = HASH_ADDRESS + 16 + size_of::<usize>();
      self.write(bucket, 1_u32);
      self.write(bucket + 4, gnu_hash(platform_symbol()) & !1);
      self.write(bucket + 8, gnu_hash(b"__vdso_clock_gettime64") | 1);
    }

    fn parse(&self) -> Option<Image> {
      // SAFETY: the test builder creates a complete in-memory native ELF image
      // whose declared readable ranges stay inside the backing allocation.
      unsafe { Image::parse(self.base()) }
    }
  }

  #[test]
  fn documented_platform_symbol_has_a_version() {
    assert!(platform_symbol().starts_with(b"__"));
    assert!(platform_version().starts_with(b"LINUX_"));
  }

  #[test]
  fn native_elf_layouts_match_the_linux_abi() {
    #[cfg(target_pointer_width = "64")]
    {
      assert_eq!(size_of::<NativeHeader>(), 64);
      assert_eq!(size_of::<NativeProgramHeader>(), 56);
      assert_eq!(size_of::<NativeDynamic>(), 16);
      assert_eq!(size_of::<NativeSymbol>(), 24);
    }
    #[cfg(target_pointer_width = "32")]
    {
      assert_eq!(size_of::<NativeHeader>(), 52);
      assert_eq!(size_of::<NativeProgramHeader>(), 32);
      assert_eq!(size_of::<NativeDynamic>(), 8);
      assert_eq!(size_of::<NativeSymbol>(), 16);
    }
    assert_eq!(size_of::<VersionDefinition>(), 20);
    assert_eq!(size_of::<VersionAuxiliary>(), 8);
  }

  #[test]
  fn elf_and_gnu_hashes_match_reference_vectors() {
    assert_eq!(elf_hash(b"LINUX_2.6"), 0x03ae_75f6);
    assert_eq!(gnu_hash(b"__vdso_clock_gettime"), 0x6e43_a318);
  }

  #[test]
  fn load_ranges_merge_when_a_later_segment_bridges_them() {
    let mut ranges = AddressRanges::new();
    ranges.insert(AddressRange::new(0x1000, 0x100).unwrap()).unwrap();
    ranges.insert(AddressRange::new(0x1200, 0x100).unwrap()).unwrap();
    ranges.insert(AddressRange::new(0x1100, 0x100).unwrap()).unwrap();
    assert_eq!(ranges.count, 1);
    assert!(ranges.contains(0x1000, 0x300));
  }

  #[test]
  fn gapped_multi_load_sysv_image_resolves_versioned_clock_symbols() {
    let image = TestImage::new(TestHash::Sysv);
    let base = image.base();
    let parsed = image.parse().expect("valid synthetic vDSO must parse");
    assert!(parsed.mapped.contains(base, HEADER_LOAD_SIZE));
    assert!(parsed.mapped.contains(base + SECOND_LOAD_ADDRESS, SECOND_LOAD_SIZE));
    assert!(!parsed.mapped.contains(base + HEADER_LOAD_SIZE, 1));
    // SAFETY: the synthetic SysV tables, symbols, versions, and strings are
    // contained in the parsed image and the lookup performs no call.
    unsafe {
      assert_eq!(
        parsed.symbol(platform_version(), platform_symbol()),
        Some(base + NATIVE_FUNCTION_ADDRESS)
      );
      assert_eq!(
        parsed.symbol(platform_version(), b"__vdso_clock_gettime64"),
        Some(base + TIME64_FUNCTION_ADDRESS)
      );
    }
  }

  #[test]
  fn gapped_multi_load_gnu_image_resolves_versioned_clock_symbols() {
    let image = TestImage::new(TestHash::Gnu);
    let base = image.base();
    let parsed = image.parse().expect("valid synthetic vDSO must parse");
    // SAFETY: the synthetic GNU tables, symbols, versions, and strings are
    // contained in the parsed image and the lookup performs no call.
    unsafe {
      assert_eq!(
        parsed.symbol(platform_version(), platform_symbol()),
        Some(base + NATIVE_FUNCTION_ADDRESS)
      );
      assert_eq!(
        parsed.symbol(platform_version(), b"__vdso_clock_gettime64"),
        Some(base + TIME64_FUNCTION_ADDRESS)
      );
      assert_eq!(parsed.symbol(b"LINUX_invalid", platform_symbol()), None);
    }
  }

  #[test]
  fn rejects_header_or_dynamic_segments_outside_declared_readable_loads() {
    let mut missing_header_load = TestImage::new(TestHash::Sysv);
    missing_header_load.write_program(
      0,
      PT_LOAD,
      PF_R | PF_X,
      0,
      0,
      size_of::<NativeHeader>(),
      HEADER_LOAD_SIZE,
    );
    assert!(missing_header_load.parse().is_none());

    let mut dynamic_in_gap = TestImage::new(TestHash::Sysv);
    let dynamic_size = DYNAMIC_COUNT * size_of::<NativeDynamic>();
    dynamic_in_gap.write_program(
      2,
      PT_DYNAMIC,
      PF_R,
      SECOND_LOAD_OFFSET,
      HEADER_LOAD_SIZE + 0x100,
      dynamic_size,
      dynamic_size,
    );
    assert!(dynamic_in_gap.parse().is_none());
  }

  #[test]
  fn rejects_unterminated_dynamic_table_and_overflowing_load() {
    let mut unterminated = TestImage::new(TestHash::Sysv);
    unterminated.write_dynamic(DYNAMIC_COUNT - 1, 1, 0);
    assert!(unterminated.parse().is_none());

    let mut overflowing = TestImage::new(TestHash::Sysv);
    overflowing.write_program(
      1,
      PT_LOAD,
      PF_R | PF_X,
      SECOND_LOAD_OFFSET,
      usize::MAX - 0x7f,
      0x100,
      0x100,
    );
    assert!(overflowing.parse().is_none());
  }

  #[test]
  fn truncated_hash_and_version_tables_fail_closed_during_lookup() {
    let mut truncated_hash = TestImage::new(TestHash::Sysv);
    truncated_hash.write_dynamic(4, DT_HASH, SECOND_LOAD_ADDRESS + SECOND_LOAD_SIZE - 1);
    let parsed = truncated_hash.parse().expect("table pointer itself is mapped");
    // SAFETY: lookup bounds the truncated table before dereferencing an entry.
    assert_eq!(unsafe { parsed.symbol(platform_version(), platform_symbol()) }, None);

    let mut truncated_version = TestImage::new(TestHash::Gnu);
    truncated_version.write_dynamic(6, DT_VERDEF, SECOND_LOAD_ADDRESS + SECOND_LOAD_SIZE - 1);
    let parsed = truncated_version.parse().expect("table pointer itself is mapped");
    // SAFETY: lookup bounds the truncated definition before reading it.
    assert_eq!(unsafe { parsed.symbol(platform_version(), platform_symbol()) }, None);
  }
}
