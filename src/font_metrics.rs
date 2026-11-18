//! Advance widths for UI layout. Fallback fonts contain long runs with identical widths.
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use std::collections::BTreeMap;
use std::fmt;

pub enum Advances {
    Ranges(Vec<(char, char, f32)>),
    Individual(BTreeMap<char, f32>),
}

impl<'de> Deserialize<'de> for Advances {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Face;
        impl<'de> Visitor<'de> for Face {
            type Value = Advances;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("character widths or ordered, disjoint character ranges")
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Advances, M::Error> {
                BTreeMap::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(Advances::Individual)
            }

            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Advances, S::Error> {
                let mut ranges = Vec::new();
                let mut previous = None;
                while let Some((first, last, width)) = seq.next_element()? {
                    if first > last || previous.is_some_and(|p| p >= first) {
                        return Err(de::Error::custom(
                            "font metric ranges overlap or are out of order",
                        ));
                    }
                    ranges.push((first, last, width));
                    previous = Some(last);
                }
                Ok(Advances::Ranges(ranges))
            }
        }
        deserializer.deserialize_any(Face)
    }
}

impl Advances {
    pub fn get(&self, character: &char) -> Option<&f32> {
        match self {
            Self::Individual(metrics) => metrics.get(character),
            Self::Ranges(ranges) => {
                let index = ranges.partition_point(|(_, last, _)| last < character);
                let (first, _, width) = ranges.get(index)?;
                (first <= character).then_some(width)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_endpoints_and_gaps_match_legacy_metrics() {
        let legacy: Advances = serde_json::from_str(r#"{"一":1,"丁":1,"七":0.75,"한":1}"#).unwrap();
        let packed: Advances =
            serde_json::from_str(r#"[["一","丁",1],["七","七",0.75],["한","한",1]]"#).unwrap();
        for c in ['A', '一', '丁', '丂', '七', '한', '힣'] {
            assert_eq!(packed.get(&c), legacy.get(&c));
        }
        assert!(serde_json::from_str::<Advances>(r#"[["b","a",1]]"#).is_err());
        assert!(serde_json::from_str::<Advances>(r#"[["a","c",1],["b","d",2]]"#).is_err());
    }
}
