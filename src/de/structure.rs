use serde::de::{DeserializeSeed, MapAccess};

use de::{Deserializer, Result};
use error::{Error, KbinErrorKind};
use node_types::StandardType;

pub struct Struct<'a, 'de: 'a> {
  de: &'a mut Deserializer<'de>,
  //fields: &'static [&'static str],
}

impl<'de, 'a> Struct<'a, 'de> {
  pub fn new(de: &'a mut Deserializer<'de>, fields: &'static [&'static str]) -> Self {
    Self {
      de,
      //fields,
    }
  }
}

impl<'de, 'a> MapAccess<'de> for Struct<'a, 'de> {
  type Error = Error;

  fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where K: DeserializeSeed<'de>
  {
    trace!("MapAccess::next_key_seed()");

    let node_type = self.de.read_node()?;
    debug!("MapAccess::next_key_seed() => node_type: {:?}", node_type);

    if node_type == StandardType::NodeEnd {
      trace!("MapAccess::next_key_seed() => end of map");
      return Ok(None);
    }

    let value = seed.deserialize(&mut *self.de).map(Some)?;

    if node_type != StandardType::NodeStart {
      // Consume the end node and do a sanity check
      let node_type = self.de.read_node()?;
      if node_type != StandardType::NodeEnd {
        return Err(KbinErrorKind::TypeMismatch(*StandardType::NodeEnd, *node_type).into());
      }
    }

    Ok(value)
  }

  fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where V: DeserializeSeed<'de>
  {
    debug!("MapAccess::next_value_seed()");
    seed.deserialize(&mut *self.de)
  }
}
