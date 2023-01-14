use clap::App;
use byteorder::{LittleEndian as L, ReadBytesExt};
use std::{
    fs::File,
    io::{self, Read},
};
use std::io::{Cursor, BufRead, BufReader, Seek, SeekFrom, BufWriter, Result};
use std::ffi::CString;
use image::{Rgb, RgbImage, Rgba, RgbaImage};

pub trait FromReader<R>
    where R : BufRead + Seek, Self : Sized
{
    fn from_reader(_: &mut R) -> Result<Self>;
}

#[derive(Debug, Clone, Copy)]
pub struct SagasColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Debug)]
struct SagasColorLUT {
    colors: Vec<SagasColor>,
}

#[derive(Debug)]
struct SagasHeader {
    unk0: u64,
    unk1: u32,
    unk2: u32,
    unk3: u32,
    unk4: u32,
    string0: CString, // source file path
    unk5: u32,
    unk6: u32,
    unk7: u32,
    image_offset: u32,
    width: u16,
    height: u16,
    unk9: u32,
    unk10: u32,
    color_table_offset: u32,
    unk12: u16,
    unk13: u16,
    unk14: u32,
    string1: CString,
}

#[derive(Debug)]
struct SagasFile {
    header: SagasHeader,
    lut: SagasColorLUT,
    image: Vec<u8>,
}

impl<R> FromReader<R> for SagasColor
    where R : BufRead + Seek
{
    fn from_reader(rd: &mut R) -> Result<Self> {
        let (r,g, b, a) = (
            rd.read_u8()?,
            rd.read_u8()?,
            rd.read_u8()?,
            rd.read_u8()?,
        );

        let a = if a != 0 {
            (((a as u16) << 1) - 1) as u8
        } else {
            a
        };

        Ok(SagasColor {
            r,
            g,
            b,
            a
        })
    }
}

impl<R> FromReader<R> for SagasColorLUT
    where R : BufRead + Seek
{
    fn from_reader(r: &mut R) -> Result<Self> {
        let num_colors = 256; // Always 256 colors?
        let mut colors = Vec::with_capacity(num_colors);
        (0..num_colors).for_each(|_| colors.push(SagasColor::from_reader(r).unwrap()));

        // Swizzle table.
        for i in (0..(num_colors)).step_by(32) {
            for (from, to) in (8..16).zip((16..24)) {
                colors.swap(i + from, i + to);
            }
        }

        Ok(SagasColorLUT {
            colors,
        })
    }
}

impl<R> FromReader<R> for CString
    where R : BufRead + Seek
{
    fn from_reader(r: &mut R) -> Result<Self> {
        let mut buffer = Vec::new();
        r.read_until(0, &mut buffer)?;
        buffer.pop();
        Ok(unsafe { CString::from_vec_unchecked(buffer) })
    }
}

impl<R> FromReader<R> for SagasHeader
    where R : BufRead + Seek
{
    fn from_reader(r: &mut R) -> Result<Self> {
        let unk0 = r.read_u64::<L>()?;
        let unk1 = r.read_u32::<L>()?;
        let unk2 = r.read_u32::<L>()?;
        let unk3 = r.read_u32::<L>()?;
        let unk4 = r.read_u32::<L>()?;
        let string0 = CString::from_reader(r)?;

        let unk5 = r.read_u32::<L>()?;
        let unk6 = r.read_u32::<L>()?;
        let unk7 = r.read_u32::<L>()?;
        let image_offset = r.read_u32::<L>()?;

        let width = r.read_u16::<L>()?;
        let height = r.read_u16::<L>()?;

        let unk9 = r.read_u32::<L>()?;
        let unk10 = r.read_u32::<L>()?;
        let color_table_offset = r.read_u32::<L>()?; // offset to beginning of color table

        let unk12 = r.read_u16::<L>()?;
        let unk13 = r.read_u16::<L>()?;
        let unk14 = r.read_u32::<L>()?;
        let string1 = CString::from_reader(r)?;

        Ok(SagasHeader {
            unk0,
            unk1,
            unk2,
            unk3,
            unk4,
            string0,
            unk5,
            unk6,
            unk7,
            image_offset,
            width,
            height,
            unk9,
            unk10,
            color_table_offset,
            unk12,
            unk13,
            unk14,
            string1,
        })
    }
}

impl<R> FromReader<R> for SagasFile
    where R : BufRead + Seek
{
    fn from_reader(r: &mut R) -> Result<Self> {
        let header = SagasHeader::from_reader(r)?;

        // Start reading the color table.
        r.seek(SeekFrom::Start((header.color_table_offset) as _));
        let lut = SagasColorLUT::from_reader(r)?;

        // Start reading the image.
        r.seek(SeekFrom::Start(header.image_offset as _));
        let mut image = Vec::with_capacity(header.width as usize * header.height as usize);
        for _ in 0..header.width * header.height {
            image.push(r.read_u8().unwrap());
        }

        Ok(SagasFile {
            header,
            lut,
            image,
        })
    }
}

impl SagasFile {
    fn get_header(&self) -> &SagasHeader {
        &self.header
    }

    fn get_color_table(&self) -> &SagasColorLUT {
        &self.lut
    }

    fn get_image(&self) -> &[u8] {
        self.image.as_slice()
    }
}

fn main() {
    let matches = App::new("dbz-sagas-extractor")
        .author("Ricky van den Waardenburg")
        .about("Extracts bitmaps from DBZ Saga indexed binary graphics format.")
        .args_from_usage(
            "-i, --input=[RAW] 'Path to binary data'
            ")
        .get_matches();

    // Read binary file.
    let path = match matches.value_of("input") {
        None => {
            println!("Missing binary file path parameter (-i, --input).");
            return;
        },
        Some(path) => path,
    };

    // Read the Sagas header.
    let bin = File::open(path);
    if bin.is_err() {
        println!("File not found.");
        return;
    }

    let mut buf_reader = BufReader::new(bin.unwrap());
    if let Ok(sf) = SagasFile::from_reader(&mut buf_reader) {
        println!("{:#?}", sf);

        let (header, image, color_table) = (sf.get_header(), sf.get_image(), sf.get_color_table());
        let (width, height) = (header.width as usize, header.height as usize);

        let mut rgba_image: RgbaImage = RgbaImage::new(width as _, height as _);
        for y in 0..height {
            for x in 0..width {
                let i = image[x + y * width] as usize;
                let c : SagasColor = color_table.colors[i];
                let (x, y) = (x as u32, y as u32);
                rgba_image.put_pixel(x, y, Rgba([c.r, c.g, c.b, c.a]));
            }
        }
        rgba_image.save(format!("out/test.png"));
    }
}