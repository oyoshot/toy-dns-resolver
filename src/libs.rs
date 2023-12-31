use std::{
    io::{Cursor, Read},
    net::{Ipv4Addr, UdpSocket},
};

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

mod consts {
    pub const DNS_BUF_SIZE: usize = 1024;
    pub const HEADER_SIZE: usize = 12;
    pub const QUESTION_DATA_SIZE: usize = 4;
    pub const RECORD_DATA_SIZE: usize = 10;
}

pub fn lookup_domain(domain_name: &str) -> Result<Ipv4Addr> {
    let query = build_query(domain_name, RecordType::A)?;

    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    socket.send_to(&query, ("8.8.8.8", 53)).unwrap();

    let mut response = [0; consts::DNS_BUF_SIZE];
    let (_, _) = socket.recv_from(&mut response).unwrap();

    let packet = parse_dns_packet(&response)?;
    let [octet1, octet2,octet3,octet4,..] = packet.answers[0].data[0..4] else { return Err(anyhow::anyhow!("data is not correct format."));};

    Ok(Ipv4Addr::new(octet1, octet2, octet3, octet4))
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

fn parse_header<const SIZE: usize>(reader: &mut Cursor<&[u8; SIZE]>) -> Result<DnsHeader> {
    let header = &mut [0; consts::HEADER_SIZE];
    reader.read_exact(header)?;
    let header: &[u8] = header;
    DnsHeader::try_from(header)
}

#[derive(Debug)]
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

fn parse_question<const SIZE: usize>(reader: &mut Cursor<&[u8; SIZE]>) -> Result<DnsQuestion> {
    let name = decord_name(reader)?;
    let data = &mut [0; consts::QUESTION_DATA_SIZE];
    reader.read_exact(data)?;
    let data: &[u8] = data;
    DnsQuestion::try_from((name.into(), data))
}

fn decord_name<const SIZE: usize>(reader: &mut Cursor<&[u8; SIZE]>) -> Result<String> {
    let mut cursor = reader.position();
    let mut parts = Vec::new();
    let mut length = reader.get_ref()[cursor as usize];

    while length != 0 {
        if (length & 0b1100_0000) != 0 {
            parts.push(decord_compressed_name(reader)?);
            cursor += 2;
            reader.set_position(cursor);
            return Ok(parts.join("."));
        } else {
            let (start, end) = ((cursor + 1) as usize, (cursor + length as u64 + 1) as usize);
            parts.push(String::from_utf8(reader.get_ref()[start..end].to_vec())?);
            cursor += length as u64 + 1;
            length = reader.get_ref()[cursor as usize];
        }
    }

    cursor += 1;
    reader.set_position(cursor);
    Ok(parts.join("."))
}

fn decord_compressed_name<const SIZE: usize>(reader: &mut Cursor<&[u8; SIZE]>) -> Result<String> {
    let curr_pos = reader.position() as usize;
    let curr = reader.get_ref()[curr_pos] & 0b0011_1111;
    let next = reader.get_ref()[curr_pos + 1];
    let cursor = u16::from_be_bytes([curr, next]);
    reader.set_position(cursor as u64);
    decord_name(reader)
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

#[derive(Debug)]
pub struct DnsRecord {
    name: Vec<u8>,
    kind: RecordType,
    class: Class,
    ttl: u32,
    data: Vec<u8>,
}

impl<const SIZE: usize> TryFrom<(Vec<u8>, &mut Cursor<&[u8; SIZE]>)> for DnsRecord {
    type Error = anyhow::Error;

    fn try_from(
        (name, reader): (Vec<u8>, &mut Cursor<&[u8; SIZE]>),
    ) -> std::result::Result<Self, Self::Error> {
        let data = &mut [0; consts::RECORD_DATA_SIZE];
        reader.read_exact(data)?;

        let kind = extract_bytes!(data, 0..2, u16);
        let class = extract_bytes!(data, 2..4, u16);
        let ttl = extract_bytes!(data, 4..8, u32);
        let data_len = extract_bytes!(data, 8..10, u16);

        let mut data = vec![0; data_len as usize];
        let _ = reader.read_exact(&mut data);

        Ok(DnsRecord {
            name,
            kind: RecordType::try_from(kind)?,
            class: Class::try_from(class)?,
            ttl,
            data,
        })
    }
}

fn parse_record<const SIZE: usize>(reader: &mut Cursor<&[u8; SIZE]>) -> Result<DnsRecord> {
    let name = decord_name(reader)?;
    DnsRecord::try_from((name.into(), reader))
}

#[derive(Debug)]
struct DnsPacket {
    pub header: DnsHeader,
    pub questions: Vec<DnsQuestion>,
    pub answers: Vec<DnsRecord>,
    pub authorities: Vec<DnsRecord>,
    pub additional: Vec<DnsRecord>,
}

fn parse_dns_packet<const SIZE: usize>(data: &[u8; SIZE]) -> Result<DnsPacket> {
    let mut reader = Cursor::new(data);

    let header = parse_header(&mut reader)?;
    let questions = (0..header.num_questions)
        .map(|_| parse_question(&mut reader))
        .collect::<Result<Vec<_>>>()?;
    let answers = (0..header.num_answers)
        .map(|_| parse_record(&mut reader))
        .collect::<Result<Vec<_>>>()?;
    let authorities = (0..header.num_authorities)
        .map(|_| parse_record(&mut reader))
        .collect::<Result<Vec<_>>>()?;
    let additional = (0..header.num_additional)
        .map(|_| parse_record(&mut reader))
        .collect::<Result<Vec<_>>>()?;

    Ok(DnsPacket {
        header,
        questions,
        answers,
        authorities,
        additional,
    })
}

#[cfg(test)]
mod test {
    use std::{
        io::Cursor,
        net::{Ipv4Addr, UdpSocket},
    };

    use crate::libs::parse_dns_packet;

    use super::{build_query, consts, encode_dns_name, header_to_bytes, DnsHeader, RecordType};

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

    #[test]
    fn send_udp_request_to_google_dns_resolver() {
        let query = build_query("www.example.com", RecordType::A).unwrap();
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        socket.send_to(&query, ("8.8.8.8", 53)).unwrap();

        let mut response = [0; consts::DNS_BUF_SIZE];
        let (_, _) = socket.recv_from(&mut response).unwrap();

        println!("Response -> {:x?}", &response);

        let r = parse_dns_packet(&response);
        println!("Parsed response -> {:?}", r);

        let data = &r.unwrap().answers[0].data;
        assert_eq!(data.len(), 4);
        assert_eq!(data[0], 93);
        assert_eq!(data[1], 184);
        assert_eq!(data[2], 216);
        assert_eq!(data[3], 34);
    }

    #[test]
    fn test_decode_name() {
        let mut buf = [0; consts::DNS_BUF_SIZE];
        buf[0] = 3;
        buf[1] = 'w' as u8;
        buf[2] = 'w' as u8;
        buf[3] = 'w' as u8;
        buf[4] = 7;
        buf[5] = 'e' as u8;
        buf[6] = 'x' as u8;
        buf[7] = 'a' as u8;
        buf[8] = 'm' as u8;
        buf[9] = 'p' as u8;
        buf[10] = 'l' as u8;
        buf[11] = 'e' as u8;
        buf[12] = 3;
        buf[13] = 'c' as u8;
        buf[14] = 'o' as u8;
        buf[15] = 'm' as u8;
        buf[16] = 0;

        let mut cur = Cursor::new(&buf);
        let name = super::decord_name(&mut cur);
        assert_eq!(name.unwrap(), "www.example.com");
        assert_eq!(cur.position(), 17);
    }
}
