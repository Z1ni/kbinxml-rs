use error::{KbinError, KbinErrorKind};

use std::fmt::Write;
use std::ops::Deref;

use byteorder::WriteBytesExt;
use failure::ResultExt;

trait KbinWrapperType<T> {
  fn from_kbin_bytes(output: &mut String, input: &[u8]) -> Result<(), KbinError>;
  fn to_kbin_bytes(output: &mut Vec<u8>, input: &str) -> Result<(), KbinError>;
}

macro_rules! number_impl {
  (integer; $($inner_type:ident),*) => {
    $(
      impl KbinWrapperType<$inner_type> for $inner_type {
        fn from_kbin_bytes(output: &mut String, input: &[u8]) -> Result<(), KbinError> {
          trace!("KbinWrapperType<{}> from bytes => input: {:02x?}", stringify!($inner_type), input);

          let mut data = [0; ::std::mem::size_of::<$inner_type>()];
          data.clone_from_slice(input);
          write!(output, "{}", $inner_type::from_be($inner_type::from_bytes(data)))
            .context(KbinErrorKind::ByteParse(stringify!($inner_type)))?;

          Ok(())
        }

        fn to_kbin_bytes(output: &mut Vec<u8>, input: &str) -> Result<(), KbinError> {
          trace!("KbinWrapperType<{}> to bytes => input: {}", stringify!($inner_type), input);

          let num = input.parse::<$inner_type>().context(KbinErrorKind::StringParse(stringify!($inner_type)))?;
          let data = $inner_type::to_bytes($inner_type::to_be(num));
          output.extend_from_slice(&data);

          Ok(())
        }
      }
    )*
  };
  (float; $($intermediate:ident => $inner_type:ident),*) => {
    $(
      impl KbinWrapperType<$inner_type> for $inner_type {
        fn from_kbin_bytes(output: &mut String, input: &[u8]) -> Result<(), KbinError> {
          trace!("KbinWrapperType<{}> from bytes => input: {:02x?}", stringify!($inner_type), input);

          let mut data = [0; ::std::mem::size_of::<$inner_type>()];
          data.clone_from_slice(input);
          let bits = $intermediate::from_be($intermediate::from_bytes(data));

          write!(output, "{:.6}", $inner_type::from_bits(bits))
            .context(KbinErrorKind::ByteParse(stringify!($inner_type)))?;

          Ok(())
        }

        fn to_kbin_bytes(output: &mut Vec<u8>, input: &str) -> Result<(), KbinError> {
          trace!("KbinWrapperType<{}> to bytes => input: {}", stringify!($inner_type), input);

          let num = input.parse::<$inner_type>().context(KbinErrorKind::StringParse(stringify!($inner_type)))?;
          let data = $intermediate::to_bytes($intermediate::to_be(num.to_bits()));
          output.extend_from_slice(&data);

          Ok(())
        }
      }
    )*
  };
}

number_impl!(integer; i8, u8, i16, u16, i32, u32, i64, u64);
number_impl!(float; u32 => f32, u64 => f64);

impl KbinWrapperType<bool> for bool {
  fn from_kbin_bytes(output: &mut String, input: &[u8]) -> Result<(), KbinError> {
    trace!("KbinWrapperType<bool> from bytes => input: {:02x?}", input);

    let value = match input[0] {
      0x00 => "0",
      0x01 => "1",
      v => panic!("Unsupported value for boolean: {}", v),
    };
    output.push_str(value);

    Ok(())
  }

  fn to_kbin_bytes(output: &mut Vec<u8>, input: &str) -> Result<(), KbinError> {
    trace!("KbinWrapperType<bool> to bytes => input: {}", input);

    let value = match input {
      "0" => 0x00,
      "1" => 0x01,
      v => panic!("Unsupported value for boolean: {}", v),
    };
    output.write_u8(value).context(KbinErrorKind::DataWrite("bool"))?;

    Ok(())
  }
}

struct Ip4;
impl KbinWrapperType<Ip4> for Ip4 {
  fn from_kbin_bytes(output: &mut String, input: &[u8]) -> Result<(), KbinError> {
    trace!("KbinWrapperType<Ip4> => input: {:02x?}", input);

    if input.len() != 4 {
      panic!("Ip4 type requires exactly 4 bytes of data, input: {:02x?}", input);
    }

    write!(output, "{}.{}.{}.{}", input[0], input[1], input[2], input[3])
      .context(KbinErrorKind::ByteParse("Ip4"))?;

    Ok(())
  }

  fn to_kbin_bytes(output: &mut Vec<u8>, input: &str) -> Result<(), KbinError> {
    trace!("KbinWrapperType<Ip4> => self: Ip4 (needs implementation!)");

    for part in input.split('.') {
      let num = part.parse::<u8>().context(KbinErrorKind::StringParse("ip4 segment"))?;
      output.write_u8(num).context(KbinErrorKind::DataWrite("ip4"))?;
    }

    Ok(())
  }
}

struct DummyConverter;
impl KbinWrapperType<DummyConverter> for DummyConverter {
  fn from_kbin_bytes(_output: &mut String, _input: &[u8]) -> Result<(), KbinError> { Ok(()) }
  fn to_kbin_bytes(_output: &mut Vec<u8>, _input: &str) -> Result<(), KbinError> { Ok(()) }
}

struct InvalidConverter;
impl KbinWrapperType<InvalidConverter> for InvalidConverter {
  fn from_kbin_bytes(_output: &mut String, input: &[u8]) -> Result<(), KbinError> {
    panic!("Invalid kbin type converter called for input: {:02x?}", input);
  }

  fn to_kbin_bytes(_output: &mut Vec<u8>, input: &str) -> Result<(), KbinError> {
    panic!("Invalid kbin type converter called for input: {}", input);
  }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct KbinType {
  pub id: u8,
  pub name: &'static str,
  pub alt_name: Option<&'static str>,
  pub size: i8,
  pub count: i8
}

impl KbinType {
  fn parse_array<T>(&self, output: &mut String, input: &[u8], arr_count: usize) -> Result<(), KbinError>
    where T: KbinWrapperType<T>
  {
    let size = self.size as usize;
    let count = self.count as usize;

    {
      let first = &input[..size];
      T::from_kbin_bytes(output, first)?;
    }

    let total_nodes = count * arr_count;
    for i in 1..total_nodes {
      let offset = i * size;
      let end = (i + 1) * size;
      let data = &input[offset..end];
      output.push(' ');
      T::from_kbin_bytes(output, data)?;
    }

    Ok(())
  }

  fn parse_bytes_inner<T>(&self, input: &[u8]) -> Result<String, KbinError>
    where T: KbinWrapperType<T>
  {
    let type_size = (self.size as usize) * (self.count as usize);
    let arr_count = input.len() / type_size;
    debug!("parse_bytes({}) => size: {}, count: {}, input_len: {}, arr_count: {}", self.name, self.size, self.count, input.len(), arr_count);

    let mut result = String::new();

    if self.count == -1 {
      panic!("Tried to parse special type: {}", self.name);
    } else if self.count == 0 {
      // Do nothing
    } else if self.count == 1 {
      // May have a node (i.e. Ip4) that is only a single count, but it
      // can be part of an array
      if arr_count == 1 {
        T::from_kbin_bytes(&mut result, input)?;
      } else {
        self.parse_array::<T>(&mut result, input, arr_count)?;
      }
    } else if self.count > 1 {
      self.parse_array::<T>(&mut result, input, arr_count)?;
    } else {
      unimplemented!();
    }

    Ok(result)
  }

  fn to_array<T>(&self, output: &mut Vec<u8>, input: &str, arr_count: usize) -> Result<(), KbinError>
    where T: KbinWrapperType<T>
  {
    Ok(())
  }

  #[allow(dead_code)]
  fn to_bytes_inner<T>(&self, data_buf: &mut Vec<u8>, input: &str, arr_count: usize) -> Result<(), KbinError>
    where T: KbinWrapperType<T>
  {
    debug!("to_bytes_inner({}) => size: {}, count: {}, input_len: {}, arr_count: {}", self.name, self.size, self.count, input.len(), arr_count);

    if self.count == -1 {
      panic!("Tried to write special type: {}", self.name);
    } else if self.count == 1 {
      // May have a node (i.e. Ip4) that is only a single count, but it
      // can be part of an array
      if arr_count == 1 {
        T::to_kbin_bytes(data_buf, input)?;
      } else {
        self.to_array::<T>(data_buf, input, arr_count)?;
      }
    } else if self.count > 1 {
      self.to_array::<T>(data_buf, input, arr_count)?;
    }

    Ok(())
  }
}

macro_rules! construct_types {
  (
    $(
      ($id:expr, $upcase:ident, $konst:ident, $name:expr, $alt_name:expr, $size:expr, $count:expr, $inner_type:ident);
    )+
  ) => {
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub enum StandardType {
      $(
        $konst,
      )+
    }

    $(
      pub const $upcase: KbinType = KbinType {
        id: $id,
        name: $name,
        alt_name: $alt_name,
        size: $size,
        count: $count,
      };
    )+

    impl StandardType {
      pub fn from_u8(input: u8) -> StandardType {
        match input {
          $(
            $id => StandardType::$konst,
          )+
          _ => panic!("Node type {} not implemented", input),
        }
      }

      pub fn from_name(input: &str) -> StandardType {
        match input {
          $(
            $name => StandardType::$konst,
          )+
          _ => panic!("Node name {} not implemented", input),
        }
      }

      pub fn parse_bytes(&self, input: &[u8]) -> Result<String, KbinError> {
        match *self {
          $(
            StandardType::$konst => self.parse_bytes_inner::<$inner_type>(input),
          )+
        }
      }

      #[allow(dead_code)]
      pub fn to_bytes(&self, output: &mut Vec<u8>, input: &str, arr_count: usize) -> Result<(), KbinError> {
        match *self {
          $(
            StandardType::$konst => self.to_bytes_inner::<$inner_type>(output, input, arr_count),
          )+
        }
      }
    }

    impl Deref for StandardType {
      type Target = KbinType;

      fn deref(&self) -> &KbinType {
        match *self {
          $(
            StandardType::$konst => &$upcase,
          )+
        }
      }
    }
  }
}

construct_types! {
  ( 2, S8,       S8,       "s8",     None,           1, 1, i8);
  ( 3, U8,       U8,       "u8",     None,           1, 1, u8);
  ( 4, S16,      S16,      "s16",    None,           2, 1, i16);
  ( 5, U16,      U16,      "u16",    None,           2, 1, u16);
  ( 6, S32,      S32,      "s32",    None,           4, 1, i32);
  ( 7, U32,      U32,      "u32",    None,           4, 1, u32);
  ( 8, S64,      S64,      "s64",    None,           8, 1, i64);
  ( 9, U64,      U64,      "u64",    None,           8, 1, u64);
  (10, BINARY,   Binary,   "bin",    Some("binary"), 1, -1, DummyConverter);
  (11, STRING,   String,   "str",    Some("string"), 1, -1, DummyConverter);
  (12, IP4,      Ip4,      "ip4",    None,           4, 1, Ip4); // Using size of 4 rather than count of 4
  (13, TIME,     Time,     "time",   None,           4, 1, u32);
  (14, FLOAT,    Float,    "float",  Some("f"),      4, 1, f32);
  (15, DOUBLE,   Double,   "double", Some("d"),      8, 1, f64);
  (16, S8_2,     S8_2,     "2s8",    None,           1, 2, i8);
  (17, U8_2,     U8_2,     "2u8",    None,           1, 2, u8);
  (18, S16_2,    S16_2,    "2s16",   None,           2, 2, i16);
  (19, U16_2,    U16_2,    "2u16",   None,           2, 2, u16);
  (20, S32_2,    S32_2,    "2s32",   None,           4, 2, i32);
  (21, U32_2,    U32_2,    "2u32",   None,           4, 2, u32);
  (22, S64_2,    S64_2,    "2s64",   Some("vs64"),   8, 2, i64);
  (23, U64_2,    U64_2,    "2u64",   Some("vu64"),   8, 2, u64);
  (24, FLOAT_2,  Float2,   "2f",     None,           4, 2, f32);
  (25, DOUBLE_2, Double2,  "2d",     Some("vd"),     8, 2, f64);
  (26, S8_3,     S8_3,     "3s8",    None,           1, 3, i8);
  (27, U8_3,     U8_3,     "3u8",    None,           1, 3, u8);
  (28, S16_3,    S16_3,    "3s16",   None,           2, 3, i16);
  (29, U16_3,    U16_3,    "3u16",   None,           2, 3, u16);
  (30, S32_3,    S32_3,    "3s32",   None,           4, 3, i32);
  (31, U32_3,    U32_3,    "3u32",   None,           4, 3, u32);
  (32, S64_3,    S64_3,    "3s64",   None,           8, 3, i64);
  (33, U64_3,    U64_3,    "3u64",   None,           8, 3, u64);
  (34, FLOAT_3,  Float3,   "3f",     None,           4, 3, f32);
  (35, DOUBLE_3, Double3,  "3d",     None,           8, 3, f64);
  (36, S8_4,     S8_4,     "4s8",    None,           1, 4, i8);
  (37, U8_4,     U8_4,     "4u8",    None,           1, 4, u8);
  (38, S16_4,    S16_4,    "4s16",   None,           2, 4, i16);
  (39, U16_4,    U16_4,    "4u16",   None,           2, 4, u16);
  (40, S32_4,    S32_4,    "4s32",   Some("vs32"),   4, 4, i32);
  (41, U32_4,    U32_4,    "4u32",   Some("vu32"),   4, 4, u32);
  (42, S64_4,    S64_4,    "4s64",   None,           8, 4, i64);
  (43, U64_4,    U64_4,    "4u64",   None,           8, 4, u64);
  (44, FLOAT_4,  Float4,   "4f",     Some("vf"),     4, 4, f32);
  (45, DOUBLE_4, Double4,  "4d",     None,           8, 4, f64);
  // 46 = Attribute
  // no 47
  (48, VS8,      Vs8,      "vs8",    None,           1, 16, i8);
  (49, VU8,      Vu8,      "vu8",    None,           1, 16, u8);
  (50, VS16,     Vs16,     "vs16",   None,           2, 8, i16);
  (51, VU16,     Vu16,     "vu16",   None,           2, 8, u16);
  (52, BOOL,     Boolean,  "bool",   Some("b"),      1, 1, bool);
  (53, BOOL_2,   Boolean2, "2b",     None,           1, 2, bool);
  (54, BOOL_3,   Boolean3, "3b",     None,           1, 3, bool);
  (55, BOOL_4,   Boolean4, "4b",     None,           1, 4, bool);
  (56, VB,       Vb,       "vb",     None,           1, 16, bool);

  ( 1, NODE_START, NodeStart, "void", None, 0, 0, InvalidConverter);
  (46, ATTRIBUTE,  Attribute, "attr", None, 0, 0, InvalidConverter);

  (190, NODE_END, NodeEnd, "nodeEnd", None, 0, 0, InvalidConverter);
  (191, FILE_END,  FileEnd, "fileEnd", None, 0, 0, InvalidConverter);
}
