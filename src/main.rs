use gstreamer as gst;
use gstreamer_app::{self as gst_app, AppSinkCallbacks};
use gstreamer_video as gst_video;
use gst::prelude::*;
use std::{env, error::Error, sync::{Arc, Mutex}};
use std::fs::File;

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize GStreamer
    gst::init()?;

    // Check for RTSP stream URI argument
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <RTSP URL>", args[0]);
        return Ok(());
    }
    let rtsp_url = &args[1];

    // Create GStreamer pipeline
    let pipeline = gstreamer::parse::launch(&format!(
        "rtspsrc location={} ! rtph264depay ! avdec_h264 ! videoconvert ! appsink name=appsink",
        rtsp_url
    ))?;

    // Get the appsink element from the pipeline
    let appsink = pipeline
        .downcast_ref::<gst::Pipeline>()
        .unwrap()
        .by_name("appsink")
        .unwrap()
        .downcast::<gst_app::AppSink>()
        .unwrap();

    // Configure the appsink
    appsink.set_caps(Some(&gst::Caps::builder("video/x-raw")
        .field("format", &"RGB")
        .build()));
    appsink.set_property("emit-signals", &true);
    appsink.set_property("sync", &false);

    // Frame counter and shared state
    let frame_counter = Arc::new(Mutex::new(0));

    // Connect to the new-sample signal of the appsink
    let frame_counter_clone = Arc::clone(&frame_counter);
    appsink.set_callbacks(
        AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = match sink.pull_sample() {
                    Ok(sample) => sample,
                    Err(_) => return Err(gst::FlowError::Error),
                };

                // Extract the buffer and caps (metadata)
                let buffer = sample.buffer().unwrap();
                let caps = sample.caps().unwrap();
                let video_info = gst_video::VideoInfo::from_caps(&caps).unwrap();

                // Convert the buffer to a readable format
                let map = buffer.map_readable().unwrap();

                // Increment the frame counter
                let mut counter = frame_counter_clone.lock().unwrap();
                *counter += 1;

                // Save frame as PNG every second (assuming 1 frame per second)
                if *counter % 30 == 0 { // Adjust based on your stream's FPS
                    let width = video_info.width() as usize;
                    let height = video_info.height() as usize;

                    // Extract the frame data
                    let frame_data = map.as_slice();

                    // Save the frame as PNG
                    let file_name = format!("frame_{}.png", *counter);
                    let file = File::create(&file_name).unwrap();
                    let mut encoder = png::Encoder::new(file, width as u32, height as u32);
                    encoder.set_color(png::ColorType::Rgb);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder.write_header().unwrap();
                    writer.write_image_data(frame_data).unwrap();

                    println!("Saved frame to {}", file_name);
                }

                Ok(gst::FlowSuccess::Ok)
            })
        .build()
    );

    // Start the pipeline
    pipeline.set_state(gst::State::Playing)?;

    // Wait until error or EOS
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Eos(..) => break,
            gst::MessageView::Error(err) => {
                eprintln!("Error from {}: {}", err.src().unwrap().path_string(), err.error());
                break;
            }
            _ => (),
        }
    }

    // Shutdown pipeline
    pipeline.set_state(gst::State::Null)?;

    Ok(())
}
