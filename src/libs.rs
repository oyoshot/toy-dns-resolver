use anyhow::{Ok, Result};
use num_enum::TryFromPrimitive;
use rand::Rng;

use crate::extract_bytes;

mod macros {
    #[macro_export]
    macro_rules! extract_bytes {
        ($buf:expr, $range:expr, $ty:tt) => {
            <$ty>::from_be_bytes($buf[$range].try_into()?)
        };
    }
}

trait ToBytes {
    fn to_bytes(&self) -> Vec<u8>;
}

#[derive(Debug, Default)]
struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub num_questions: u16,
    pub num_answers: u16,
    pub num_authorities: u16,
    pub num_additional: u16,
}

impl ToBytes for DnsHeader {
    fn to_bytes(&self) -> Vec<u8> {
        [
            self.id.to_be_bytes(),
            self.flags.to_be_bytes(),
            self.num_questions.to_be_bytes(),
            self.num_answers.to_be_bytes(),
            self.num_authorities.to_be_bytes(),
            self.num_additional.to_be_bytes(),
        ]
        .concat()
    }
}

impl TryFrom<&[u8]> for DnsHeader {
    type Error = anyhow::Error;

    fn try_from(buf: &[u8]) -> Result<Self> {
        Ok(DnsHeader {
            id: extract_bytes!(buf, 0..2, u16),
            flags: extract_bytes!(buf, 2..4, u16),
            num_questions: extract_bytes!(buf, 4..6, u16),
            num_answers: extract_bytes!(buf, 6..8, u16),
            num_authorities: extract_bytes!(buf, 8..10, u16),
            num_additional: extract_bytes!(buf, 10..12, u16),
        })
    }
}

struct DnsQuestion {
    pub name: Vec<u8>,
    pub kind: RecordType,
    pub class: Class,
}

impl ToBytes for DnsQuestion {
    fn to_bytes(&self) -> Vec<u8> {
        [
            self.name.clone(),
            (self.kind.clone() as u16).to_be_bytes().to_vec(),
            (self.class.clone() as u16).to_be_bytes().to_vec(),
        ]
        .concat()
    }
}

impl TryFrom<(Vec<u8>, &[u8])> for DnsQuestion {
    type Error = anyhow::Error;

    fn try_from((name, data): (Vec<u8>, &[u8])) -> std::result::Result<Self, Self::Error> {
        Ok(DnsQuestion {
            name,
            kind: RecordType::try_from(extract_bytes!(data, 0..2, u16))?,
            class: Class::try_from(extract_bytes!(data, 2..4, u16))?,
        })
    }
}

fn build_query(domain_name: &str, record_type: RecordType) -> Result<Vec<u8>> {
    let name = encode_dns_name(domain_name)?;
    let id = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..=65535)
    };
    let recursion_desired = 1 << 8;
    let header = DnsHeader {
        id,
        flags: recursion_desired,
        num_questions: 1,
        ..Default::default()
    };
    let question = DnsQuestion {
        name,
        kind: record_type,
        class: Class::In,
    };

    let mut bytes = header_to_bytes(header);
    bytes.extend_from_slice(&question_to_bytes(question));

    Ok(bytes)
}

fn header_to_bytes(header: DnsHeader) -> Vec<u8> {
    header.to_bytes()
}

fn question_to_bytes(question: DnsQuestion) -> Vec<u8> {
    question.to_bytes()
}

fn encode_dns_name(name: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for label in name.split('.') {
        bytes.push(label.len() as u8);
        bytes.extend_from_slice(label.as_bytes());
    }
    bytes.push(0);
    Ok(bytes)
}

#[derive(Debug, Default, Clone, TryFromPrimitive)]
#[repr(u16)]
pub enum RecordType {
    #[default]
    A = 1,
}

#[derive(Debug, Default, Clone, TryFromPrimitive)]
#[repr(u16)]
pub enum Class {
    #[default]
    In = 1,
}

#[cfg(test)]
mod test {
    use super::{build_query, encode_dns_name, header_to_bytes, DnsHeader, RecordType};

    #[test]
    fn test_header() {
        let h = DnsHeader {
            id: 0x1314,
            flags: 000000000,
            num_questions: 1,
            num_answers: 0,
            num_authorities: 0,
            num_additional: 0,
        };
        let h = header_to_bytes(h);
        println!("DNS Header -> {:02x?}", h)
    }

    #[test]
    fn test_encode_dns_name() {
        let e = encode_dns_name("google.com").unwrap();
        assert_eq!(e[0], 6);
        assert_eq!(e[7], 3);
    }

    #[test]
    fn test_build_query() {
        let q = build_query("www.example.com", RecordType::A).unwrap();
        println!("Build Query -> {:02x?}", q)
    }
}
