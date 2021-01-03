// Generated by packetc v0.1.0 at Sun, 03 Jan 2021 19:52:55 +0000
#![allow(non_camel_case_types, unused_imports, clippy::field_reassign_with_default)]
extern crate packet;
use std::convert::TryFrom;
pub type uint8 = u8;
pub type uint16 = u16;
pub type uint32 = u32;
pub type int8 = i8;
pub type int16 = i16;
pub type int32 = i32;
pub type float = f32;
pub type string = String;
#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum Flag {
    A = 1 << 0,
    B = 1 << 1,
}
impl Default for Flag {
    fn default() -> Self {
        Flag::A
    }
}
impl std::convert::TryFrom<u8> for Flag {
    type Error = packet::Error;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Flag::A),
            2 => Ok(Flag::B),
            _ => Err(packet::Error::InvalidEnumValue(value.to_string(), "Flag")),
        }
    }
}
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ComplexType {
    pub flag: Flag,
    pub positions: Vec<Position>,
    pub names: Vec<string>,
    pub values: Vec<Value>,
}
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Value {
    pub a: uint32,
    pub b: int32,
    pub c: uint8,
    pub d: uint8,
}
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Position {
    pub x: float,
    pub y: float,
}
pub fn read(reader: &mut packet::reader::Reader, output: &mut ComplexType) -> Result<(), packet::Error> {
    output.flag = Flag::try_from(reader.read_uint8()?)?;
    let output_positions_len = reader.read_uint32()? as usize;
    output.positions.reserve(output_positions_len);
    for _ in 0..output_positions_len {
        let mut output_positions_item = Position::default();
        output_positions_item.x = reader.read_float()?;
        output_positions_item.y = reader.read_float()?;
        output.positions.push(output_positions_item);
    }
    let output_names_len = reader.read_uint32()? as usize;
    output.names.reserve(output_names_len);
    for _ in 0..output_names_len {
        let output_names_item_len = reader.read_uint32()? as usize;
        output
            .names
            .push(reader.read_string(output_names_item_len)?.to_string());
    }
    let output_values_len = reader.read_uint32()? as usize;
    output.values.reserve(output_values_len);
    for _ in 0..output_values_len {
        let mut output_values_item = Value::default();
        output_values_item.a = reader.read_uint32()?;
        output_values_item.b = reader.read_int32()?;
        output_values_item.c = reader.read_uint8()?;
        output_values_item.d = reader.read_uint8()?;
        output.values.push(output_values_item);
    }
    Ok(())
}
pub fn write(writer: &mut packet::writer::Writer, input: &ComplexType) {
    writer.write_uint8(input.flag as u8);
    writer.write_uint32(input.positions.len() as u32);
    for input_positions_item in input.positions.iter() {
        writer.write_float(input_positions_item.x);
        writer.write_float(input_positions_item.y);
    }
    writer.write_uint32(input.names.len() as u32);
    for input_names_item in input.names.iter() {
        writer.write_uint32(input_names_item.len() as u32);
        writer.write_string(&input_names_item);
    }
    writer.write_uint32(input.values.len() as u32);
    for input_values_item in input.values.iter() {
        writer.write_uint32(input_values_item.a);
        writer.write_int32(input_values_item.b);
        writer.write_uint8(input_values_item.c);
        writer.write_uint8(input_values_item.d);
    }
}

#[cfg(test)]
mod tests {
    use packet::{reader::Reader, writer::Writer};

    use super::*;

    #[test]
    fn writes() {
        let test = ComplexType {
            flag: Flag::B,
            positions: vec![Position { x: 0.0, y: 1.0 }],
            names: vec!["first".to_string(), "second".to_string()],
            values: vec![Value {
                a: 0u32,
                b: 1i32,
                c: 30u8,
                d: 100u8,
            }],
        };

        let expected: &[u8] = &[
            2, // flag
            1, 0, 0, 0, // positions.len()
            0, 0, 0, 0, // positions[0].x
            0, 0, 128, 63, // positions[0].y
            2, 0, 0, 0, // names.len()
            5, 0, 0, 0, // names[0].len()
            102, 105, 114, 115, 116, // names[0][0..5]
            6, 0, 0, 0, // names[1].len()
            115, 101, 99, 111, 110, 100, // names[1][0..6]
            1, 0, 0, 0, // values.len()
            0, 0, 0, 0, // values[0].a
            1, 0, 0, 0,   // values[0].b
            30,  // values[0].c
            100, // values[0].d
        ];

        let mut writer = Writer::new();
        write(&mut writer, &test);
        assert_eq!(&writer.finish(), expected);
    }

    #[test]
    fn reads() {
        let test: &[u8] = &[
            2, // flag
            1, 0, 0, 0, // positions.len()
            0, 0, 0, 0, // positions[0].x
            0, 0, 128, 63, // positions[0].y
            2, 0, 0, 0, // names.len()
            5, 0, 0, 0, // names[0].len()
            102, 105, 114, 115, 116, // names[0][0..5]
            6, 0, 0, 0, // names[1].len()
            115, 101, 99, 111, 110, 100, // names[1][0..6]
            1, 0, 0, 0, // values.len()
            0, 0, 0, 0, // values[0].a
            1, 0, 0, 0,   // values[0].b
            30,  // values[0].c
            100, // values[0].d
        ];

        let expected = ComplexType {
            flag: Flag::B,
            positions: vec![Position { x: 0.0, y: 1.0 }],
            names: vec!["first".to_string(), "second".to_string()],
            values: vec![Value {
                a: 0u32,
                b: 1i32,
                c: 30u8,
                d: 100u8,
            }],
        };

        let mut reader = Reader::new(test);
        let mut actual = ComplexType::default();
        read(&mut reader, &mut actual).unwrap();
        assert_eq!(actual, expected);
    }
}

fn main() {}
