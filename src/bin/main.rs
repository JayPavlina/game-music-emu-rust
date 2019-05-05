#![allow(dead_code, unused_imports, unused_variables, unused_must_use, unused_mut, non_camel_case_types)]
#![feature(maybe_uninit)]
//extern crate libc;

use std::io::Read;
use game_music_emu::native::*;
use game_music_emu::EmuHandle;


fn main() {
    do_header();
    do_get_types();
    do_open();
}


fn do_header() {
    let mut file = std::fs::File::open("assets/smb3.nsf").unwrap();
    let mut buffer: Vec<u8> = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    let header = identify_header(buffer);
    println!("header {}", header);
}

fn do_open() {
    let mut file = std::fs::File::open("assets/smb3.nsf").unwrap();
    let mut buffer: Vec<u8> = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    let length = buffer.len();
    let mut track_count = 0;
    unsafe {
        let mut emu_ptr = std::mem::MaybeUninit::uninit().as_mut_ptr();
//        let mut emu_ptr = std::ptr::null_mut();
//        emu_ptr = *1;
        let error = gme_open_data(buffer.as_ptr(), length, emu_ptr, 44100);
        track_count = get_track_count(&EmuHandle::new(*emu_ptr));
    }
    println!("track count {}", track_count);
}

fn do_get_types() {
   println!("types: {:?}", get_types());
}





//
//#[repr(C)]
//struct Buffer {
//    data: *mut u8,
//    len: usize,
//}
//
//fn generate_data() -> Buffer {
//    let mut file = std::fs::File::open("assets/smb3.nsf").unwrap();
//    let mut buffer = Vec::new();
//    file.read_to_end(&mut buffer).expect("bad");
//    let mut boxedBuffer = vec![0; 512].into_boxed_slice();
//    let data = boxedBuffer.as_mut_ptr();
//    let len = boxedBuffer.len();
//    std::mem::forget(boxedBuffer);
//    Buffer { data, len }
//}



//fn gme() {

//    let byte_count = file.bytes().count();
//    println!("File has {} bytes", byte_count);
//}

//fn rodio() {
//    let device = rodio::default_output_device().unwrap();
//    let sink = rodio::Sink::new(&device);
//
//    let file = std::fs::File::open("assets/bgm_simon_overworld.ogg").unwrap();
//    sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());
//
//    sink.sleep_until_end();
//}

//fn run() -> std::io::Result<()> {
//    let path = env::current_dir()?;
//    println!("The current directory is {}", path.display());
//    Ok(())
//}