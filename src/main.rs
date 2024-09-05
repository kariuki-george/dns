use std::io::{BufRead, Cursor, Read};
#[allow(unused_imports)]
use std::net::UdpSocket;

use anyhow::{Context, Result};
use bitreader::BitReader;
use bitvec::{prelude::*, view::BitView};
use bytes::Buf;
use rust_bitwriter::BitWriter;

fn main() {
    println!("Logs from your program will appear here!");

    let udp_socket = UdpSocket::bind("127.0.0.1:2053").expect("Failed to bind to address");
    let mut buf = [0; 512];

    loop {
        match udp_socket.recv_from(&mut buf) {
            Ok((size, source)) => {
                println!("Received {} bytes from {}", size, source);

                let mut message = Message::new(buf);
                println!("{:?}", message);

                let response = message.write();
                println!("{:?}", response);

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
    questions: Vec<Question>,

    authority: String,
    space: String,
}

impl Message {
    fn new(buf: [u8; 512]) -> Self {
        let mut message = Message {
            ..Default::default()
        };
        let mut cursor = Cursor::new(buf);
        message.header = Header::parse(&mut cursor).unwrap();

        for _ in 0..message.header.question_count {
            let question = Question::parse(&mut cursor).unwrap();
            message.questions.push(question);
        }

        message
    }

    fn write(&mut self) -> Vec<u8> {
        let mut message: [u8; 512] = [0; 512];
        self.header.answer_record_count = self.header.question_count;
        self.header.qr_indicator = true;
        self.header.r_code = if self.header.opcode == 0 { 0 } else { 4 };
        let header = self.header.write().unwrap();
        for (index, byte) in header.iter().enumerate() {
            message[index] = byte.to_owned()
        }

        let mut position = 12;
        for question in &self.questions {
            let question = question.write(false).unwrap();
            for byte in question {
                message[position] = byte.to_owned();
                position += 1;
            }
        }
        for question in &self.questions {
            let question = question.write(true).unwrap();
            for byte in question {
                message[position] = byte.to_owned();
                position += 1;
            }
        }

        message[0..position].to_vec()
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
        header.question_count = reader.read_u16(16)?;
        header.answer_record_count = reader.read_u16(16)?;
        header.authoritative_record_count = reader.read_u16(16)?;
        header.additional_record_count = reader.read_u16(16)?;

        Ok(header)
    }

    fn write(&self) -> Result<[u8; 12]> {
        let mut writer = BitWriter::new();

        writer.write_u16(self.packet_id, 16)?;
        writer.write_bool(self.qr_indicator)?;
        writer.write_u8(self.opcode, 4)?;
        writer.write_bool(self.aa)?;
        writer.write_bool(self.truncation)?;
        writer.write_bool(self.recursion_desired)?; // recursion_desired
        writer.write_bool(false)?; //recursion_available
        writer.write_u8(0, 3)?; // reserved
        writer.write_u8(self.r_code, 4)?; // r_code
        writer.write_u16(self.question_count, 16)?; // question count
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

#[derive(Debug, Default)]
struct Question {
    names: Vec<String>,
    q_type: u16,
    class: u16,
}

impl Question {
    fn parse(cursor: &mut Cursor<[u8; 512]>) -> Result<Self> {
        let mut buffer = Vec::new();

        cursor
            .read_until(0, &mut buffer)
            .context("Failed to read question buffer")?;

        // Parse labels
        let mut label_cursor = Cursor::new(buffer);

        let mut question = Question {
            ..Default::default()
        };

        let mut labels = Vec::new();
        loop {
            let position = label_cursor.position() as usize;
            if label_cursor.get_ref()[position] == b'\0' {
                break;
            }
            let length = label_cursor.get_u8();

            let mut label_buf: Vec<u8> = vec![0; length.into()];

            label_cursor
                .read_exact(&mut label_buf)
                .context("Failed to read label buffer")?;

            let label = String::from_utf8(label_buf)?;
            labels.push(label);
        }
        question.names = labels;
        let q_type = cursor.get_u16();
        let class = cursor.get_u16();
        question.q_type = q_type;
        question.class = class;

        Ok(question)
    }
    fn write(&self, is_answer: bool) -> Result<Vec<u8>> {
        let mut question = Vec::new();
        for label in &self.names {
            let length: u8 = label
                .len()
                .try_into()
                .context("Question label was more than 255 characters long")?;

            question.push(length);
            question = [question, label.as_bytes().to_vec()].concat();
        }
        question.push(b'\0');
        question = [question, self.q_type.to_be_bytes().to_vec()].concat();
        question = [question, self.class.to_be_bytes().to_vec()].concat();
        println!("{:?}", question);

        if is_answer {
            let ttl: u32 = 60;
            let length: u16 = 4;
            let data: [u8; 4] = [10; 4];

            question = [question, ttl.to_be_bytes().to_vec()].concat();
            question = [question, length.to_be_bytes().to_vec()].concat();

            for section in data {
                question.push(section);
            }
        }

        println!("{:?}", question);

        Ok(question)
    }
}
