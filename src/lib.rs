#![feature(int_to_from_bytes)]

extern crate byteorder;
extern crate encoding;
extern crate minidom;
extern crate num;
extern crate rustc_hex;

#[macro_use] extern crate failure;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;

use std::cmp::max;
use std::fmt::Write as FmtWrite;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use failure::ResultExt;
use minidom::Element;

mod compression;
mod encoding_type;
mod error;
mod node_types;
mod sixbit;

use compression::Compression;
use encoding_type::EncodingType;
use node_types::{KbinType, StandardType};
use sixbit::{pack_sixbit, unpack_sixbit};
use rustc_hex::FromHex;

pub use error::{KbinError, KbinErrorKind};

const SIGNATURE: u8 = 0xA0;

const SIG_COMPRESSED: u8 = 0x42;

pub struct KbinXml {
  offset_1: u64,
  offset_2: u64,
}

impl KbinXml {
  pub fn new() -> Self {
    Self {
      offset_1: 0,
      offset_2: 0,
    }
  }

  #[inline]
  fn data_buf_offset(&self, data_buf: &Cursor<&[u8]>) -> u64 {
    // Position is not the index of the previously read byte, it is the current
    // index (offset).
    //
    // This is so much fun to debug.
    //data_buf.position() - 1
    data_buf.position()
  }

  fn data_buf_read(&mut self, data_buf: &mut Cursor<&[u8]>) -> Result<Vec<u8>, KbinError> {
    let size = data_buf.read_u32::<BigEndian>().context(KbinErrorKind::DataReadSize)?;
    let mut data = vec![0; size as usize];
    data_buf.read_exact(&mut data).context(KbinErrorKind::DataRead)?;
    trace!("data_buf_read => size: {}, data: 0x{:02x?}", data.len(), data);

    self.data_buf_realign_reads(data_buf, None)?;

    Ok(data)
  }

  fn data_buf_read_str(&mut self, data_buf: &mut Cursor<&[u8]>, encoding: EncodingType) -> Result<String, KbinError> {
    let mut data = self.data_buf_read(data_buf)?;

    // Remove trailing null bytes
    let mut index = data.len() - 1;
    let len = data.len();
    while index < len && data[index] == 0x00 {
      index -= 1;
    }
    data.truncate(index + 1);
    trace!("data_buf_read_str => size: {}, data: 0x{:02x?}", data.len(), data);

    encoding.decode_bytes(data)
  }

  fn data_buf_get(&mut self, data_buf: &mut Cursor<&[u8]>, size: u32) -> Result<Vec<u8>, KbinError> {
    let mut data = vec![0; size as usize];
    data_buf.read_exact(&mut data).context(KbinErrorKind::DataRead)?;

    Ok(data)
  }

  fn data_buf_get_aligned(&mut self, data_buf: &mut Cursor<&[u8]>, data_type: KbinType) -> Result<Vec<u8>, KbinError> {
    if self.offset_1 % 4 == 0 {
      self.offset_1 = self.data_buf_offset(data_buf);
    }
    if self.offset_2 % 4 == 0 {
      self.offset_2 = self.data_buf_offset(data_buf);
    }

    let old_pos = self.data_buf_offset(data_buf);
    let size = data_type.size * data_type.count;
    trace!("data_buf_get_aligned => old_pos: {}, size: {}", old_pos, size);
    let (check_old, data) = match size {
      1 => {
        data_buf.seek(SeekFrom::Start(self.offset_1)).context(KbinErrorKind::Seek)?;

        let data = data_buf.read_u8().context(KbinErrorKind::DataReadOneByte)?;
        self.offset_1 += 1;

        (true, vec![data])
      },
      2 => {
        data_buf.seek(SeekFrom::Start(self.offset_2)).context(KbinErrorKind::Seek)?;

        let mut data = vec![0; 2];
        data_buf.read_exact(&mut data).context(KbinErrorKind::DataReadTwoByte)?;
        self.offset_2 += 2;

        (true, data)
      },
      size => {
        let mut data = vec![0; size as usize];
        data_buf.read_exact(&mut data).context(KbinErrorKind::DataReadAligned)?;
        self.data_buf_realign_reads(data_buf, None)?;

        (false, data)
      },
    };


    if check_old {
      data_buf.seek(SeekFrom::Start(old_pos)).context(KbinErrorKind::Seek)?;

      let trailing = max(self.offset_1, self.offset_2);
      trace!("data_buf_get_aligned => old_pos: {}, trailing: {}", old_pos, trailing);
      if old_pos < trailing {
        data_buf.seek(SeekFrom::Start(trailing)).context(KbinErrorKind::Seek)?;
        self.data_buf_realign_reads(data_buf, None)?;
      }
    }

    Ok(data)
  }

  fn data_buf_realign_reads(&mut self, data_buf: &mut Cursor<&[u8]>, size: Option<u64>) -> Result<(), KbinError> {
    let size = size.unwrap_or(4);
    trace!("data_buf_realign => position: {}, size: {}", data_buf.position(), size);

    while data_buf.position() % size > 0 {
      data_buf.seek(SeekFrom::Current(1)).context(KbinErrorKind::Seek)?;
    }
    trace!("data_buf_realign => realigned to: {}", data_buf.position());

    Ok(())
  }

  fn from_binary_internal(&mut self, input: &[u8]) -> Result<Element, KbinError> {
    // Node buffer starts from the beginning.
    // Data buffer starts later after reading `len_data`.
    let mut node_buf = Cursor::new(&input[..]);

    let signature = node_buf.read_u8().context(KbinErrorKind::SignatureRead)?;
    assert_eq!(signature, SIGNATURE);

    // TODO: support uncompressed
    let compress_byte = node_buf.read_u8().context(KbinErrorKind::CompressionRead)?;
    assert_eq!(compress_byte, SIG_COMPRESSED);

    let compressed = Compression::from_byte(compress_byte)?;

    let encoding_byte = node_buf.read_u8().context(KbinErrorKind::EncodingRead)?;
    let encoding_negation = node_buf.read_u8().context(KbinErrorKind::EncodingNegationRead)?;
    let encoding = EncodingType::from_byte(encoding_byte)?;
    assert_eq!(encoding_negation, 0xFF ^ encoding_byte);

    info!("signature: 0x{:x}", signature);
    info!("compression: 0x{:x} ({:?})", compress_byte, compressed);
    info!("encoding: 0x{:x} ({:?})", encoding_byte, encoding);

    let len_node = node_buf.read_u32::<BigEndian>().context(KbinErrorKind::LenNodeRead)?;
    info!("len_node: {} (0x{:x})", len_node, len_node);

    // We have read 8 bytes so far, so offset the start of the data buffer from
    // our current position.
    let data_buf_start = len_node + 8;
    let mut data_buf = Cursor::new(&input[(data_buf_start as usize)..]);

    {
      let pos = data_buf.position();
      self.offset_1 = pos;
      self.offset_2 = pos;
      trace!("offset_1: {}, offset_2: {}", self.offset_1, self.offset_2);
    }

    let len_data = data_buf.read_u32::<BigEndian>().context(KbinErrorKind::LenDataRead)?;
    info!("len_data: {} (0x{:x})", len_data, len_data);

    let mut stack: Vec<Element> = Vec::new();
    {
      let node_buf_end = data_buf_start.into();
      while node_buf.position() < node_buf_end {
        let raw_node_type = node_buf.read_u8().context(KbinErrorKind::NodeTypeRead)?;
        let is_array = raw_node_type & 64 == 64;
        let node_type = raw_node_type & !64;

        let xml_type = StandardType::from_u8(node_type);
        debug!("raw_node_type: {}, node_type: {:?} ({}), is_array: {}", raw_node_type, xml_type, node_type, is_array);

        match xml_type {
          StandardType::NodeEnd | StandardType::FileEnd => {
            if stack.len() > 1 {
              let node = stack.pop().expect("Stack must have last node");
              if let Some(to) = stack.last_mut() {
                to.append_child(node);
              }
            }

            if xml_type == StandardType::NodeEnd {
              continue;
            } else if xml_type == StandardType::FileEnd {
              break;
            }
          },
          _ => {},
        };

        let name = unpack_sixbit(&mut node_buf)?;

        if xml_type == StandardType::NodeStart {
          stack.push(Element::bare(name));
        } else {
          if xml_type != StandardType::Attribute {
            stack.push(Element::bare(name.clone()));
          }
          if let Some(to) = stack.last_mut() {
            match xml_type {
              StandardType::Attribute => {
                let val = self.data_buf_read_str(&mut data_buf, encoding)?;
                debug!("attr name: {}, val: {}", name, val);
                to.set_attr(name, val);
              },
              // Removing null bytes is *so much* fun.
              //
              // Handle String nodes separately to use the string reading logic
              // which automatically removes trailing null bytes.
              StandardType::String => {
                to.set_attr("__type", xml_type.name);

                let val = self.data_buf_read_str(&mut data_buf, encoding)?;
                debug!("name: {}, val: {}", name, val);
                to.append_text_node(val);
              },
              _ => {
                to.set_attr("__type", xml_type.name);

                let type_size = xml_type.size;
                let type_count = xml_type.count;
                let (is_array, size) = if type_count == -1 {
                  (true, data_buf.read_u32::<BigEndian>().context(KbinErrorKind::BinaryLengthRead)?)
                } else if is_array {
                  let node_size = type_size * type_count;
                  let arr_count = data_buf.read_u32::<BigEndian>().context(KbinErrorKind::ArrayLengthRead)? / node_size as u32;
                  to.set_attr("__count", arr_count);

                  let size = (node_size as u32) * arr_count;
                  (true, size)
                } else {
                  (false, 1)
                };

                debug!("type: {:?}, type_size: {}, type_count: {}, is_array: {}, size: {}",
                  xml_type,
                  type_size,
                  type_count,
                  is_array,
                  size);

                let data = if is_array {
                  let data = self.data_buf_get(&mut data_buf, size)?;
                  self.data_buf_realign_reads(&mut data_buf, None)?;

                  data
                } else {
                  self.data_buf_get_aligned(&mut data_buf, *xml_type)?
                };

                debug!("data: 0x{:02x?}", data);
                if xml_type == StandardType::Binary {
                  to.set_attr("__size", data.len());

                  let len = data.len() * 2;
                  let val = data.into_iter().fold(String::with_capacity(len), |mut val, x| {
                    write!(val, "{:02x}", x).expect("Failed to append hex char");
                    val
                  });
                  debug!("name: {}, string: {}", name, val);
                  to.append_text_node(val);
                } else {
                  let inner_value = xml_type.parse_bytes(&data)?;
                  debug!("name: {}, string: {}", name, inner_value);
                  to.append_text_node(inner_value);
                }
              },
            };
          }
        }
      }
    }
    if stack.len() > 1 {
      warn!("stack: {:#?}", stack);
    }
    stack.truncate(1);
    Ok(stack.pop().expect("Stack must have root node"))
  }

  fn write_node<W>(&mut self, node_buf: &mut W, data_buf: &mut W, input: &Element) -> Result<(), KbinError>
    where W: Write
  {
    let text = input.text();
    let node_type = match input.attr("__type") {
      Some(name) => StandardType::from_name(name),
      None => {
        if text.len() == 0 {
          StandardType::NodeStart
        } else {
          StandardType::String
        }
      },
    };

    let (array_mask, count) = match input.attr("__count") {
      Some(count) => {
        let array_mask = 1 << 6;
        let count = count.parse::<i8>().context(KbinErrorKind::StringParse("array count"))?;
        (array_mask, count)
      },
      None => {
        (0, 0)
      },
    };

    println!("input name: {}", input.name());

    node_buf.write_u8(node_type.id | array_mask).context(KbinErrorKind::DataWrite(node_type.name))?;
    pack_sixbit(node_buf, input.name())?;

    match node_type {
      StandardType::NodeStart => {},

      StandardType::Binary => {
        let bin = text.from_hex();
        println!("data: {:?}", bin);
      },
      StandardType::String => {
        println!("str: {}", text);
      },

      _ => {
      },
    }

    for child in input.children() {
      self.write_node(node_buf, data_buf, child)?;
    }

    // Always has the array bit set
    node_buf.write_u8(StandardType::NodeEnd.id | 64).context(KbinErrorKind::DataWrite("node end"))?;

    Ok(())
  }

  fn to_binary_internal(&mut self, input: &Element) -> Result<Vec<u8>, KbinError> {
    let mut header = Cursor::new(Vec::with_capacity(8));
    header.write_u8(SIGNATURE).context(KbinErrorKind::HeaderWrite("signature"))?;
    header.write_u8(SIG_COMPRESSED).context(KbinErrorKind::HeaderWrite("compression"))?;

    let encoding = EncodingType::SHIFT_JIS.to_byte();
    header.write_u8(encoding).context(KbinErrorKind::HeaderWrite("encoding"))?;
    header.write_u8(0xFF ^ encoding).context(KbinErrorKind::HeaderWrite("encoding negation"))?;

    let mut node_buf = Cursor::new(Vec::new());
    let mut data_buf = Cursor::new(Vec::new());

    self.write_node(&mut node_buf, &mut data_buf, input)?;

    node_buf.write_u8(StandardType::FileEnd.id | 64).context(KbinErrorKind::DataWrite("file end"))?;

    let mut output = header.into_inner();

    let node_buf = node_buf.into_inner();
    output.write_u32::<BigEndian>(node_buf.len() as u32).context(KbinErrorKind::HeaderWrite("node buffer length"))?;
    output.extend_from_slice(&node_buf);

    let data_buf = data_buf.into_inner();
    output.write_u32::<BigEndian>(data_buf.len() as u32).context(KbinErrorKind::HeaderWrite("data buffer length"))?;
    output.extend_from_slice(&data_buf);

    Ok(output)
  }

  pub fn from_binary(input: &[u8]) -> Result<Element, KbinError> {
    let mut kbinxml = KbinXml::new();
    kbinxml.from_binary_internal(input)
  }

  pub fn to_binary(input: &Element) -> Result<Vec<u8>, KbinError> {
    let mut kbinxml = KbinXml::new();
    kbinxml.to_binary_internal(input)
  }
}
