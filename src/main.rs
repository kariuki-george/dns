use std::io::{Cursor, Read};
#[allow(unused_imports)]
use std::net::UdpSocket;

use anyhow::Result;
use bitreader::BitReader;
use bitvec::{prelude::*, view::BitView};
use rust_bitwriter::BitWriter;

fn main() {
    println!("Logs from your program will appear here!");

    let udp_socket = UdpSocket::bind("127.0.0.1:2053").expect("Failed to bind to address");
    let mut buf = [0; 512];

    loop {
        match udp_socket.recv_from(&mut buf) {
            Ok((size, source)) => {
                println!("Received {} bytes from {}", size, source);

                let message = Message::new(buf);
                println!("{:?}", message);

                let response = message.write();
                udp_socket
                    .send_to(&response, source)
                    .expect("Failed to send response");
            }
            Err(e) => {
                eprintln!("Error receiving data: {}", e);
                break;
            }
        }
    }
}
#[derive(Debug, Default)]
struct Message {
    header: Header,
    question: String,
    answer: String,
    authority: String,
    space: String,
}

impl Message {
    fn new(buf: [u8; 512]) -> Self {
        let mut cursor = Cursor::new(buf);
        let header = Header::parse(&mut cursor).unwrap();

        Message {
            header,
            ..Default::default()
        }
    }

    fn write(&self) -> [u8; 512] {
        let header = self.header.write().unwrap();

        let mut message: [u8; 512] = [0; 512];

        for (index, byte) in header.iter().enumerate() {
            message[index] = byte.to_owned()
        }

        message
    }
}

#[derive(Debug, Default)]
struct Header {
    packet_id: u16,
    qr_indicator: bool,
    opcode: u8,
    aa: bool,
    truncation: bool,
    recursion_desired: bool,
    recursion_available: bool,
    reserved: u8,
    r_code: u8,
    question_count: u16,
    answer_record_count: u16,
    authoritative_record_count: u16,
    additional_record_count: u16,
}

impl Header {
    fn parse(cursor: &mut Cursor<[u8; 512]>) -> Result<Self> {
        let mut buf: [u8; 12] = [0; 12];

        cursor.read_exact(&mut buf)?;

        let mut reader = BitReader::new(&buf);

        let mut header = Header {
            ..Default::default()
        };
        header.packet_id = reader.read_u16(16)?;
        header.qr_indicator = reader.read_bool()?;
        header.opcode = reader.read_u8(4)?;
        header.aa = reader.read_bool()?;
        header.truncation = reader.read_bool()?;
        header.recursion_desired = reader.read_bool()?;
        header.recursion_available = reader.read_bool()?;
        header.reserved = reader.read_u8(3)?;
        header.r_code = reader.read_u8(4)?;
        header.question_count = reader.read_u16(2)?;
        header.answer_record_count = reader.read_u16(2)?;
        header.authoritative_record_count = reader.read_u16(2)?;
        header.additional_record_count = reader.read_u16(2)?;

        Ok(header)
    }

    fn write(&self) -> Result<[u8; 12]> {
        let mut writer = BitWriter::new();

        writer.write_u16(self.packet_id, 16)?;
        writer.write_bool(true)?;
        writer.write_u8(0, 4)?;
        writer.write_bool(self.aa)?;
        writer.write_bool(self.truncation)?;
        writer.write_bool(false)?; // recursion_desired
        writer.write_bool(false)?; //recursion_available
        writer.write_u8(0, 3)?; // reserved
        writer.write_u8(0, 4)?; // r_code
        writer.write_u16(0, 16)?; // question count
        writer.write_u16(self.answer_record_count, 16)?;
        writer.write_u16(self.authoritative_record_count, 16)?;
        writer.write_u16(self.additional_record_count, 16)?;
        writer.close()?;
        let buffer = writer.data().to_owned();

        let mut header: [u8; 12] = [0; 12];
        for (index, byte) in buffer.iter().enumerate() {
            header[index] = byte.to_owned();
        }
        Ok(header)
    }
}

fn u16_to_u8s(input: u16) -> (u8, u8) {
    let upper_byte = (input >> 8) as u8;
    let lower_byte = input as u8;

    (upper_byte, lower_byte)
}
