use std::{fs::OpenOptions, io::Write, process::exit, time::Duration};

use libcamera::{
    camera::CameraConfigurationStatus, camera_manager::CameraManager, framebuffer::AsFrameBuffer, framebuffer_allocator::{FrameBuffer, FrameBufferAllocator}, framebuffer_map::MemoryMappedFrameBuffer, geometry::Size, pixel_format::PixelFormat, properties, request::ReuseFlag, stream::StreamRole
};

// drm-fourcc does not have MJPEG type yet, construct it from raw fourcc identifier
const PIXEL_FORMAT_RGB888: PixelFormat = PixelFormat::new(u32::from_le_bytes([b'R', b'G', b'2', b'4']), 0);

fn main() {
    let filename = match std::env::args().nth(1) {
        Some(f) => f,
        None => {
            println!("Error: missing file output parameter");
            println!("Usage: ./video_capture </path/to/output.mjpeg>");
            exit(1);
        }
    };

    let mgr = CameraManager::new().unwrap();

    let cameras = mgr.cameras();

    let cam = cameras.get(0).expect("No cameras found");

    println!(
        "Using camera: {}",
        *cam.properties().get::<properties::Model>().unwrap()
    );

    let mut cam = cam.acquire().expect("Unable to acquire camera");

    // This will generate default configuration for each specified role
    let mut cfgs = cam.generate_configuration(&[StreamRole::VideoRecording]).unwrap();

    cfgs.get_mut(0).unwrap().set_pixel_format(PIXEL_FORMAT_RGB888);
    cfgs.get_mut(0).unwrap().set_size(Size {
        width: 640,
        height: 480
    });

    println!("Generated config: {:#?}", cfgs);

    match cfgs.validate() {
        CameraConfigurationStatus::Valid => println!("Camera configuration valid!"),
        CameraConfigurationStatus::Adjusted => println!("Camera configuration was adjusted: {:#?}", cfgs),
        CameraConfigurationStatus::Invalid => panic!("Error validating camera configuration"),
    }

    // Ensure that pixel format was unchanged
    assert_eq!(
        cfgs.get(0).unwrap().get_pixel_format(),
        PIXEL_FORMAT_RGB888,
        "MJPEG is not supported by the camera"
    );

    cam.configure(&mut cfgs).expect("Unable to configure camera");
    println!("Used config: {:#?}", cfgs);

    let mut alloc = FrameBufferAllocator::new(&cam);

    // Allocate frame buffers for the stream
    let cfg = cfgs.get(0).unwrap();
    let stream = cfg.stream().unwrap();
    let buffers = alloc.alloc(&stream).unwrap();
    println!("Allocated {} buffers", buffers.len());

    // Convert FrameBuffer to MemoryMappedFrameBuffer, which allows reading &[u8]
    let buffers = buffers
        .into_iter()
        .map(|buf| MemoryMappedFrameBuffer::new(buf).unwrap())
        .collect::<Vec<_>>();

    // Create capture requests and attach buffers
    let reqs = buffers
        .into_iter()
        .enumerate()
        .map(|(i, buf)| {
            let mut req = cam.create_request(Some(i as u64)).unwrap();
            req.add_buffer(&stream, buf).unwrap();
            req
        })
        .collect::<Vec<_>>();

    // Completed capture requests are returned as a callback
    let (tx, rx) = std::sync::mpsc::channel();
    cam.on_request_completed(move |req| {
        tx.send(req).unwrap();
    });

    // TODO: Set `Control::FrameDuration()` here. Blocked on https://github.com/lit-robotics/libcamera-rs/issues/2
    cam.start(None).unwrap();

    // Enqueue all requests to the camera
    for req in reqs {
        println!("Request queued for execution: {req:#?}");
        cam.queue_request(req).unwrap();
    }

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&filename)
        .expect("Unable to create output file");
    
    for _ in 0..5 {
        println!("Waiting for camera request execution");
        let mut req = rx.recv().expect("Sender disconnect");

        println!("Camera request {:?} completed!", req);
        println!("Metadata: {:#?}", req.metadata());

        // Get framebuffer for our stream
        let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap(); //type framebuffer == the same as when it was created
        let metadata = framebuffer.metadata().unwrap(); //В этот момент метадата заведомо существует 
        println!("FrameBuffer metadata: {:#?}", metadata);

        // MJPEG format has only one data plane containing encoded jpeg data with all the headers
        let planes = framebuffer.data();
        let frame_data = planes.get(0).unwrap(); //?для формата rg24 только 1 plane

        print!("{}", planes.len());
        // Actual encoded data will be smalled than framebuffer size, its length can be obtained from metadata.
        let bytes_used = metadata.planes().get(0).unwrap().bytes_used as usize;//?для формата rg24 только 1 plane

        file.write(&frame_data[..bytes_used]).unwrap();
        println!("Written {} bytes to {}", bytes_used, &filename);

        // Recycle the request back to the camera for execution
        req.reuse(ReuseFlag::REUSE_BUFFERS);
        cam.queue_request(req).unwrap();//мы получаем точно известные буферы

    }   

    // Everything is cleaned up automatically by Drop implementations
}