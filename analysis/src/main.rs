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
    println!("Reading NMEA file took: {:?}", start.elapsed());
    lines
}

fn parse_nmea(nmea_sentences: Vec<String>, config: &config::Config) -> Vec<db::record::Record> {
    let start = std::time::Instant::now();
    let records = nmea::nmea_to_records(nmea_sentences, config);
    println!("NMEA parsing took: {:?}", start.elapsed());
    records
}

fn find_arcs(records: &VecDeque<db::record::Record>) -> Vec<db::arc::Arc> {
    let start = std::time::Instant::now();
    let arcs = gnssir::find_arcs(records);
    println!("Finding arcs took: {:?}", start.elapsed());
    arcs
}

fn process_arcs(arcs: &Vec<db::arc::Arc>, records: &mut VecDeque<db::record::Record>) {
    let start = std::time::Instant::now();
    for arc in arcs {
        gnssir::fix_arc_elev_azim(arc, records);
    }
    println!("Fixing arc elevation and azimuth took: {:?}", start.elapsed());
    let start = std::time::Instant::now();
    for arc in arcs {
        gnssir::correct_arc_snr(arc, records);
    }
    println!("Correcting arc SNR took: {:?}", start.elapsed());
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
        return;

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

fn main() {
    let start: std::time::Instant = std::time::Instant::now();
    let config: config::Config = config::Config::default();
    let mut record_db: db::record::RecordDatabase = db::record::RecordDatabase::new();

    let nmea_sentences = read_nmea_file("data/nmea2.txt");
    let records = parse_nmea(nmea_sentences, &config);

    println!("Parsed {} records from NMEA sentences.", records.len());

    record_db.insert_many(records);

    // for record in &record_db.records {
    //     println!(
    //         "Record - ID: {:05}, Network: {:1}, Band: {:1}, Elevation: {:>2}, Azimuth: {:>3}, SNR: {:>2}, Time: {:>5}",
    //         record.id,
    //         record.network as u32,
    //         record.band as u32,
    //         record.elevation,
    //         record.azimuth,
    //         record.snr,
    //         record.time
    //     );
    // }

    println!("Database now contains {} records, with size {} KB", record_db.len(), record_db.check_memory()/(1024));
    
    let arcs = find_arcs(&record_db.records);
    // println!("Found {} arcs in the records.", arcs.len());
    
    process_arcs(&arcs, &mut record_db.records);

    // let mut wtr = start_csv("results/records.csv", &["id", "time", "network", "band", "elevation", "azimuth", "snr"]);
    // for record in &record_db.records {
    //     write_to_csv(
    //         &mut wtr,
    //         &[
    //             record.id.to_string(),
    //             record.time.to_string(),
    //             format!("{:?}", record.network),
    //             format!("{:?}", record.band),
    //             record.elevation.to_string(),
    //             record.azimuth.to_string(),
    //             record.snr.to_string(),
    //         ],
    //     );
    // }
    // flush_csv(&mut wtr);

    find_results(&arcs, &record_db.records, &config);
    // println!("Total runtime: {:?}", start.elapsed());
}
