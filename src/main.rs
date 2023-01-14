use clap::App;
use byteorder::{LittleEndian as L, ReadBytesExt};
use std::{
    fs::File,
    io::{self, Read},
};
use std::io::{Cursor, BufRead, BufReader, Seek, SeekFrom, BufWriter};
use std::ffi::CString;
use image::{Rgb, RgbImage, Rgba, RgbaImage};

#[derive(Debug, Clone, Copy)]
pub struct SagasColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl SagasColor {
    fn from_reader<R: BufRead>(rd: &mut R) -> io::Result<Self> {
        let ( r,g, b, a) = (
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

#[derive(Debug)]
struct SagasColorLUT {
    unk0: u32,
    colors: Vec<SagasColor>,
}

impl SagasColorLUT {
    fn from_reader<R: BufRead>(r: &mut R, num_colors: usize) -> io::Result<Self> {
        let unk0 = r.read_u32::<L>()?; // number of colors?
        let mut colors = Vec::with_capacity(unk0 as _);
        (0..num_colors).for_each(|_| colors.push(SagasColor::from_reader(r).unwrap()));

        // Swizzle table.
        for i in (0..(num_colors)).step_by(32) {
            for (from, to) in (8..16).zip((16..24)) {
                colors.swap(i + from, i + to);
            }
        }

        Ok(SagasColorLUT {
            unk0,
            colors,
        })
    }
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
    unk8: u32,
    width: u16,
    height: u16,
    unk9: u32,
    unk10: u32,
    unk11: u32, // palette width?
    unk12: u16, // palette width?
    unk13: u16, // palette height?
    unk14: u32, // palette width?
    string1: CString,
}

impl SagasHeader {
    fn from_reader<R: BufRead>(r: &mut R) -> io::Result<Self> {
        let unk0 = r.read_u64::<L>()?;
        let unk1 = r.read_u32::<L>()?; // offset to color table?
        let unk2 = r.read_u32::<L>()?;
        let unk3 = r.read_u32::<L>()?;
        let unk4 = r.read_u32::<L>()?;

        let mut string0 = Vec::new();
        r.read_until(0, &mut string0)?;
        string0.pop();
        let string0 = unsafe { CString::from_vec_unchecked(string0) };

        let unk5 = r.read_u32::<L>()?;
        let unk6 = r.read_u32::<L>()?;
        let unk7 = r.read_u32::<L>()?; //
        let unk8 = r.read_u32::<L>()?; // offset to beginning of image

        let width = r.read_u16::<L>()?;
        let height = r.read_u16::<L>()?;

        let unk9 = r.read_u32::<L>()?;
        let unk10 = r.read_u32::<L>()?;
        let unk11 = r.read_u32::<L>()?; // offset to beginning of color table

        let unk12 = r.read_u16::<L>()?;
        let unk13 = r.read_u16::<L>()?;
        let unk14 = r.read_u32::<L>()?;

        let mut string1 = Vec::new();
        r.read_until(0, &mut string1)?;
        string1.pop();
        let string1 = unsafe { CString::from_vec_unchecked(string1) };

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
            unk8,
            width,
            height,
            unk9,
            unk10,
            unk11,
            unk12,
            unk13,
            unk14,
            string1,
        })
    }
}

#[derive(Debug)]
struct SagasFile {
    header: SagasHeader,
    lut: SagasColorLUT,
    image: Vec<u8>,
}

impl SagasFile {
    fn from_reader<R: BufRead + Seek>(r: &mut R, color_offset: usize) -> io::Result<Self> {
        let header = SagasHeader::from_reader(r)?;

        // Move reader to correct color table offset.
        r.seek(SeekFrom::Start((header.unk11 - 4) as _));
        let lut = SagasColorLUT::from_reader(r, (header.unk12 * header.unk13) as _)?;

        // Move reader to image offset.
        r.seek(SeekFrom::Start(header.unk8 as _));
        let mut buffer = Vec::with_capacity(header.width as usize * header.height as usize);
        for _ in 0..header.width * header.height {
            buffer.push(r.read_u8().unwrap());
        }

        Ok(SagasFile {
            header,
            lut,
            image: buffer,
        })
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
    let sf = SagasFile::from_reader(&mut buf_reader, 0).unwrap();
    println!("{:#?}", sf);

    let mut image: RgbaImage = RgbaImage::new(sf.header.width as _, sf.header.height as _);
    for y in 0..sf.header.height as usize {
        for x in 0..sf.header.width as usize {
            let idx = sf.image[x + y * sf.header.width as usize];
            let color: SagasColor = sf.lut.colors[idx as usize];
            image.put_pixel(x as u32, y as u32, Rgba([color.r, color.g, color.b, color.a]));
        }
    }

    image.save(format!("out/test.png"));
}