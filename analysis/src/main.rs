use std::{time::Duration};
use std::collections::VecDeque;

use csv::Writer;



mod db;
mod nmea;
mod config;
mod gnssir;
mod math;

fn read_nmea_file(file_path: &str) -> Vec<String> {
    let start = std::time::Instant::now();
    let lines = std::fs::read_to_string(file_path)
        .expect("Failed to read NMEA file")
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<String>>();
    // println!("Reading NMEA file took: {:?}", start.elapsed());
    lines
}

fn parse_nmea(nmea_sentences: Vec<String>, config: &config::Config) -> Vec<db::record::Record> {
    let start = std::time::Instant::now();
    let records = nmea::nmea_to_records(nmea_sentences, config);
    // println!("NMEA parsing took: {:?}", start.elapsed());
    records
}

fn find_arcs(records: &VecDeque<db::record::Record>) -> Vec<db::arc::Arc> {
    let start = std::time::Instant::now();
    let arcs = gnssir::find_arcs(records);
    // println!("Finding arcs took: {:?}", start.elapsed());
    arcs
}

fn process_arcs(arcs: &Vec<db::arc::Arc>, records: &mut VecDeque<db::record::Record>) {
    let start = std::time::Instant::now();
    for arc in arcs {
        gnssir::fix_arc_elev_azim(arc, records);
    }
    // println!("Fixing arc elevation and azimuth took: {:?}", start.elapsed());
    let start = std::time::Instant::now();
    for arc in arcs {
        gnssir::correct_arc_snr(arc, records);
    }
    // println!("Correcting arc SNR took: {:?}", start.elapsed());
}

fn start_csv(file_path: &str, headers: &[&str]) -> Writer<std::fs::File> {
    let mut wtr = Writer::from_path(file_path).expect("Failed to create CSV file");
    wtr.write_record(headers).expect("Failed to write header");
    wtr
}

fn write_to_csv(wtr: &mut Writer<std::fs::File>, record: &[String]) {
    wtr.write_record(record).expect("Failed to write record");
}

fn flush_csv(wtr: &mut Writer<std::fs::File>) {
    wtr.flush().expect("Failed to flush CSV writer");
}

fn find_results(arcs: &Vec<db::arc::Arc>, records: &VecDeque<db::record::Record>, config: &config::Config) {
    let mut wtr = start_csv("results/arc_freqs.csv", &["i", "id", "frequency", "amplitude", "num"]);

    let mut freqs: Vec<Vec<(f64, f64)>> = Vec::new();

    let start = std::time::Instant::now();
    for id in 0..arcs.len() {
        let arc = &arcs[id];
        let start2 = std::time::Instant::now();
        let frequencies = gnssir::find_arc_frequencies(arc, records, &config);
        let duration2 = start2.elapsed();
        println!("Arc ID {}: Found {} frequency components in {:?}", arc.sat_id, frequencies.len(), duration2);

        for (freq, amp) in &frequencies {
            write_to_csv(&mut wtr, &[id.to_string(), arc.sat_id.to_string(), freq.to_string(), amp.to_string(), arc.record_indices.len().to_string()]);
        }
        freqs.push(frequencies);
    }
    println!("Frequency analysis took: {:?}", start.elapsed());

    flush_csv(&mut wtr);

    let start = std::time::Instant::now();
    
    for (arc, frequencies) in arcs.iter().zip(freqs.iter()) {
        if let Some((freq, amp)) = gnssir::find_max_amplitude_frequency(frequencies) {
            let mean_elev = arc.record_indices.iter()
                .filter_map(|&idx| records.get(idx).map(|rec| rec.elevation))
                .sum::<f64>() / arc.record_indices.len() as f64;
            let mean_azim = arc.record_indices.iter()
                .filter_map(|&idx| records.get(idx).map(|rec| rec.azimuth))
                .sum::<f64>() / arc.record_indices.len() as f64;
            let mean_ampl = frequencies.iter().map(|(_,a)| *a).sum::<f64>() / frequencies.len() as f64;
            let median_time = {
                let mut times: Vec<i64> = arc.record_indices.iter()
                    .filter_map(|&idx| records.get(idx).map(|rec| rec.time))
                    .collect();
                times.sort();
                times[times.len() / 2]
            };

            println!("Arc ID {}: Max amplitude frequency {:.4} with amplitude {:.4} (mean: {:.4}) at mean elev {:.2}, azim {:.2}, median time {}, num records {}",
                arc.sat_id, freq, amp, mean_ampl, mean_elev, mean_azim, median_time, arc.record_indices.len());
        }
    }
    println!("Collecting results took: {:?}", start.elapsed());
}

fn run_analysis(nmea_file: &str) -> (Duration, Duration) {
    let config = config::Config::default();

    let start = std::time::Instant::now();
    let nmea_sentences = read_nmea_file(nmea_file);
    let records_vec = parse_nmea(nmea_sentences, &config);
    let mut records: VecDeque<db::record::Record> = VecDeque::from(records_vec);
    let duration_read_parse = start.elapsed();

    let start = std::time::Instant::now();
    let arcs = find_arcs(&records);
    process_arcs(&arcs, &mut records);
    let duration_process = start.elapsed();

    (duration_read_parse, duration_process)
}

const FOLDER_PATH: &str = "data2/";

fn main() {
    let mut t1 = Vec::<u32>::new();
    let mut t2 = Vec::<u32>::new();
    let mut iterations = 5;
    for i in 0..5 {
        let mut t1_sum = 0u32;
        let mut t2_sum = 0u32;
        for file in std::fs::read_dir(FOLDER_PATH).expect("Failed to read data folder") {
            let file = file.expect("Failed to read file in data folder");
            let file_path = file.path();
            if file_path.is_file() {
                println!("Processing file: {:?}", file_path);
                let (duration_read_parse, duration_process) = run_analysis(file_path.to_str().unwrap());
                t1_sum += duration_read_parse.as_millis() as u32;
                t2_sum += duration_process.as_millis() as u32;
            }
        }
        t1.push(t1_sum);
        t2.push(t2_sum);
    }

    let mut t1_sum = t1.iter().sum::<u32>();
    let mut t2_sum = t2.iter().sum::<u32>();

    let t1_mean = t1_sum as f32 / iterations as f32;
    let t2_mean = t2_sum as f32 / iterations as f32;

    let t1_std = (t1.iter().map(|&x| (x as f32 - t1_mean).powi(2)).sum::<f32>() / iterations as f32).sqrt();
    let t2_std = (t2.iter().map(|&x| (x as f32 - t2_mean).powi(2)).sum::<f32>() / iterations as f32).sqrt();

    let total_sum = t1.iter().zip(t2.iter()).map(|(&x, &y)| x + y).sum::<u32>();
    let total_mean = total_sum as f32 / iterations as f32;
    let total_std = (t1.iter().zip(t2.iter())
        .map(|(&x, &y)| {
            let total = x + y;
            (total as f32 - total_mean).powi(2)
        })
        .sum::<f32>() / iterations as f32).sqrt();

    println!("Average read & parse time: {:.2} ms ± {:.2} ms", t1_mean, t1_std);
    println!("Average process time: {:.2} ms ± {:.2} ms", t2_mean, t2_std);
    println!("Average total time: {:.2} ms ± {:.2} ms", total_mean, total_std);
}
