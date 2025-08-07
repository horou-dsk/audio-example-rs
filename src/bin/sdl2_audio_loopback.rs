use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use sdl2::{event::Event, keyboard::Keycode, pixels::Color, rect::Rect};
use spectrum_analyzer::{
    samples_fft_to_spectrum, scaling::divide_by_N_sqrt, windows::hann_window, FrequencyLimit,
};

fn map_range(value: f32, from_range: (f32, f32), to_range: (f32, f32)) -> f32 {
    let from_min = from_range.0;
    let from_max = from_range.1;
    let to_min = to_range.0;
    let to_max = to_range.1;

    // 确保 value 在 from_range 范围内
    let clamped_value = value.max(from_min).min(from_max);

    // 计算映射后的值
    ((clamped_value - from_min) / (from_max - from_min)) * (to_max - to_min) + to_min
}

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    let width = 1920;
    let height = 600;

    let window = video_subsystem
        .window("rust-sdl2 demo: Audio", width, height)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();
    let mut event_pump = sdl_context.event_pump()?;

    let (tx, rx) = crossbeam::channel::unbounded::<(Vec<f32>, Vec<f32>)>();
    let host = cpal::default_host();
    let devices = host.devices().unwrap();
    for device in devices {
        println!("{:?}", device.name());
    }
    let device = host.default_output_device().unwrap();
    println!("default device name = {:?}", device.name());
    let mut supported_configs_range = device.supported_output_configs().unwrap();
    let config = supported_configs_range
        .next()
        .unwrap()
        .with_max_sample_rate();
    println!("{config:#?}");
    // let mut supported_configs_range = device.supported_output_configs().unwrap();
    // let config = supported_configs_range
    //     .next()
    //     .unwrap()
    //     .with_max_sample_rate();
    let rate = config.sample_rate();
    const BUFFER_SIZE: usize = 960 * 2;
    let mut buffer_data = [0.0; BUFFER_SIZE];
    let mut buffer_data_index = 0;
    let stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _info| {
                // println!(
                //     "{:?} {:?}",
                //     info.timestamp().capture,
                //     info.timestamp().callback
                // );
                // 采样数值大小跟播放音量有关系
                // println!("{:?}", data.len());
                buffer_data[buffer_data_index..buffer_data_index + data.len()]
                    .copy_from_slice(data);
                buffer_data_index += data.len();
                if buffer_data_index >= BUFFER_SIZE {
                    buffer_data_index = 0;
                    tx.send(lr_fr(&buffer_data, rate.0)).unwrap();
                }

                // println!("{:?}", &data[..20]);
            },
            |err| {
                eprintln!("stream error {err:?}");
            },
            None,
        )
        .unwrap();
    stream.play().unwrap();

    let mut old = [0.0; 1000];
    let mut new = [0.0; 1000];
    let mut current = [0.0; 1000];

    let line_num = 480;
    let channel_num = line_num / 2;
    let rw = (width as usize) / line_num;
    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }
        if let Ok((left_fr, right_fr)) = rx.try_recv() {
            if left_fr.len() >= channel_num && right_fr.len() >= channel_num {
                old.copy_from_slice(&current);
                for (i, val) in left_fr.iter().take(channel_num).rev().enumerate() {
                    new[i] = *val;
                }
                // new[..channel_num].copy_from_slice(&left_fr[..channel_num]);
                new[channel_num..channel_num + channel_num]
                    .copy_from_slice(&right_fr[..channel_num]);
            }
        }
        canvas.set_draw_color(Color::RGB(16, 29, 43));
        canvas.clear();

        canvas.set_draw_color(Color::RGB(123, 0, 0));
        for i in 0..line_num {
            current[i] += (new[i] - old[i]) / 10.0;
            let height = map_range(current[i], (0.0, 1.0), (0.0, 500.0)) as u32;
            let y = 600 - height;
            let rect = Rect::new((i * rw) as i32, y as i32, rw as u32, height);
            canvas.fill_rect(rect)?;
            canvas.draw_rect(rect)?;
        }
        // if let Ok((left_fr, right_fr)) = rx.try_recv() {
        //     let line_num = 480;
        //     let channel_num = line_num / 2;
        //     if left_fr.len() > channel_num && right_fr.len() > channel_num {
        //         canvas.set_draw_color(Color::RGB(16, 29, 43));
        //         canvas.clear();

        //         canvas.set_draw_color(Color::RGB(123, 0, 0));
        //         let rw = (width as usize) / line_num;
        //         for i in 0..line_num {
        //             let fr = if i >= channel_num {
        //                 right_fr[i - channel_num]
        //             } else {
        //                 left_fr[channel_num - 1 - i]
        //             };
        //             let height = map_range(fr, (0.0, 1.0), (0.0, 500.0)) as u32;
        //             let y = 600 - height;
        //             let rect = Rect::new(i as i32 * rw as i32, y as i32, rw as u32, height);
        //             canvas.fill_rect(rect)?;
        //             canvas.draw_rect(rect)?;
        //         }
        //     }
        // }

        canvas.present();
        ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 144));
        // The rest of the game loop goes here...
    }
    Ok(())
}

fn lr_fr(samples: &[f32], rate: u32) -> (Vec<f32>, Vec<f32>) {
    let left_samples: Vec<f32> = samples
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, v)| *v)
        .collect();
    let right_samples: Vec<f32> = samples
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, v)| *v)
        .collect();
    let left_len = left_samples.len();
    let right_len = right_samples.len();
    (
        samples_to_fr(left_samples, left_len, rate),
        samples_to_fr(right_samples, right_len, rate),
    )
}

fn samples_to_fr(mut f32_samples: Vec<f32>, num_samples: usize, rate: u32) -> Vec<f32> {
    let next_power_of_two = (num_samples as f32).log2().ceil() as usize;
    let new_num_samples = 2usize.pow(next_power_of_two as u32);
    f32_samples.resize(new_num_samples, 0.0);
    let f32_samples = hann_window(&f32_samples);
    let spectrum_hann_window = samples_fft_to_spectrum(
        // (windowed) samples
        &f32_samples,
        // sampling rate
        rate,
        // optional frequency limit: e.g. only interested in frequencies 50 <= f <= 150?
        FrequencyLimit::Min(250.0),
        // optional scale
        Some(&divide_by_N_sqrt),
    )
    .unwrap();
    let mut freq_data: Vec<f32> = spectrum_hann_window
        .data()
        .iter()
        .map(|(hz, fr_val)| {
            let mut amplitude = fr_val.val() * 50.0;
            let hz = hz.val();
            // 频率加权，降低低频的影响
            let frequency_weight = (hz / 5000.0).clamp(0.05, 1.0);
            amplitude *= frequency_weight;

            amplitude
        })
        .collect();
    // 应用平滑滤波，减少突变
    if freq_data.len() > 2 {
        for i in 1..freq_data.len() - 1 {
            freq_data[i] = (freq_data[i - 1] + freq_data[i] + freq_data[i + 1]) / 3.0;
        }
    }
    freq_data
}
