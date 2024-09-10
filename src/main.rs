use std::net::UdpSocket;
use std::usize;
use std::{
    env::args,
    io::{Cursor, Read},
};

use anyhow::{Context, Result};
use bitreader::BitReader;
use bytes::Buf;
use rust_bitwriter::BitWriter;

fn main() {
    println!("Logs from your program will appear here!");

    let udp_socket = UdpSocket::bind("0.0.0.0:2053").expect("Failed to bind to address");
    let mut buf = [0; 512];

    let args = args();
    let mut address = String::new();

    if args.len() > 1 {
        // Parse forwarding server
        let addr = args
            .into_iter()
            .last()
            .expect("Expected forwarding server addr");
        address = addr.clone();
        let _ = addr
            .split_once(':')
            .expect("Expected address to be of <addr>:<port> format");
    }

    loop {
        match udp_socket.recv_from(&mut buf) {
            Ok((_size, source)) => {
                let message = Message::new(buf, false);

                let mut response = message.clone().write(true);
                if !address.is_empty() {
                    // let mut message = forward_query(&udp_socket, &address, message).unwrap();
                    let mut message = make_codecrafters_happy(&udp_socket, &address, message);
                    response = message.write(true);
                }

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

fn make_codecrafters_happy(udp: &UdpSocket, addr: &str, message: Message) -> Message {
    println!("{:?}", message);
    let mut answer_questions = vec![];
    let mut response = Message::default();

    for question in 0..message.header.question_count {
        let mut to_send_question = Message::default();
        to_send_question.header = message.header.clone();
        to_send_question.header.packet_id = question;
        to_send_question.header.question_count = 1;
        to_send_question
            .questions
            .push(message.questions[question as usize].clone());

        println!("To push question. {:?}", to_send_question);

        let response = forward_query(udp, addr, to_send_question).unwrap();

        println!("\nsome response. {:?}", response);

        answer_questions.push(response);
    }
    // Assumes all went well
    response.header = message.header.clone();
    response.header.answer_record_count = message.header.question_count;
    response.header.qr_indicator = true;
    println!("{:?}", answer_questions);
    for answer in 0..message.header.question_count {
        let answer = answer_questions[0].clone();
        response.questions = [response.questions, answer.questions].concat();
        response.answers = [response.answers, answer.answers].concat();
    }

    println!("Response {:?}", response);
    response
}

fn forward_query(udp: &UdpSocket, addr: &str, mut message: Message) -> Result<Message> {
    let query = message.write(false);
    udp.send_to(&query, addr)
        .context("Failed to forward message")?;
    let mut buf = [0; 512];

    udp.recv_from(&mut buf)?;

    let message = Message::new(buf, true);
    Ok(message)
}

#[derive(Debug, Default, Clone)]
struct Message {
    header: Header,
    questions: Vec<Question>,
    answers: Vec<Question>,
}

impl Message {
    fn new(buf: [u8; 512], is_answer: bool) -> Self {
        let mut message = Message {
            ..Default::default()
        };
        let mut cursor = Cursor::new(buf);
        message.header = Header::parse(&mut cursor).unwrap();
        //Read questions
        for _ in 0..message.header.question_count {
            let question = Question::parse(&mut cursor, false).unwrap();

            message.questions.push(question);
        }

        // Read Answers

        if is_answer {
            for _ in 0..message.header.answer_record_count {
                let answer = Question::parse(&mut cursor, true).unwrap();
                message.answers.push(answer);
            }
        }

        message
    }

    fn write(&mut self, is_response: bool) -> Vec<u8> {
        let mut message: [u8; 512] = [0; 512];
        if is_response {
            self.header.answer_record_count = self.header.question_count;
            self.header.qr_indicator = true;
            self.header.r_code = if self.header.opcode == 0 { 0 } else { 4 };
        }
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
        for question in &self.answers {
            let question = question.write(true).unwrap();
            for byte in question {
                message[position] = byte.to_owned();
                position += 1;
            }
        }

        message[0..position].to_vec()
    }
}

#[derive(Debug, Default, Clone)]
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

#[derive(Debug, Default, Clone)]
struct Question {
    names: Vec<String>,
    q_type: u16,
    class: u16,
    ttl: u32,
    length: u16,
    data: Vec<u8>,
}

impl Question {
    fn parse(cursor: &mut Cursor<[u8; 512]>, is_answer: bool) -> Result<Self> {
        let question = parse_question(cursor, is_answer)?;

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

        if is_answer {
            question = [question, self.ttl.to_be_bytes().to_vec()].concat();
            question = [question, self.length.to_be_bytes().to_vec()].concat();

            for section in self.data.clone() {
                question.push(section);
            }
        }

        Ok(question)
    }
}
fn parse_label(cursor: &mut Cursor<[u8; 512]>) -> Result<(String, Question)> {
    // Check if pointer
    let octet_1 = cursor.get_u8();
    let octet_1 = [octet_1];
    let mut reader = BitReader::new(&octet_1);
    let distinguisher = reader.read_u8(2)?;
    if distinguisher == 3 {
        // Compressed
        let octet_2 = cursor.get_u8();
        let offset = [reader.read_u8(6)?, octet_2];
        let offset = u16::from_be_bytes(offset);
        let cursor_position = cursor.position();
        cursor.set_position(offset.into());
        let question = parse_labels(cursor, Question::default())?;
        cursor.set_position(cursor_position);

        Ok((String::new(), question))
    } else {
        //NOTE: Other including Uncompressed and reserved 01, 10
        //Implements only the uncompressed
        let length = u8::from_be_bytes(octet_1);
        let mut label_buf: Vec<u8> = vec![0; length.into()];

        cursor
            .read_exact(&mut label_buf)
            .context("Failed to read label buffer")?;

        let label = String::from_utf8(label_buf)?;
        Ok((label, Question::default()))
    }
}

fn parse_question(cursor: &mut Cursor<[u8; 512]>, is_answer: bool) -> Result<Question> {
    let question = Question::default();
    let mut question = parse_labels(cursor, question)?;
    let q_type = cursor.get_u16();
    let class = cursor.get_u16();
    question.q_type = q_type;
    question.class = class;
    if is_answer {
        question.ttl = cursor.get_u32();
        question.length = cursor.get_u16();
        let mut data: Vec<u8> = vec![0; question.length.into()];
        for index in 0..question.length {
            data[index as usize] = cursor.get_u8();
        }
        question.data = data;
    }

    Ok(question)
}

fn parse_labels(cursor: &mut Cursor<[u8; 512]>, mut question: Question) -> Result<Question> {
    loop {
        let position = cursor.position() as usize;
        if cursor.get_ref()[position] == b'\0' {
            break;
        }

        let label = parse_label(cursor)?;
        if label.0.is_empty() {
            // cursor.get_u8();
            return Ok(label.1);
        }
        question.names.push(label.0);
    }
    // Read the null byte
    cursor.get_u8();

    Ok(question)
}
