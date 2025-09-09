use csv::Writer;

mod db;
mod nmea;
mod config;
mod gnssir;

fn main() {
    let config: config::Config = config::Config::default();
    let mut record_db: db::record::RecordDatabase = db::record::RecordDatabase::new();

    let nmea_sentences = std::fs::read_to_string("data/nmea1.txt")
        .expect("Failed to read data/nmea1.txt")
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<String>>();

let start = std::time::Instant::now();
    let records = nmea::nmea_to_records(nmea_sentences, &config);
let duration = start.elapsed();
    println!("NMEA parsing took: {:?}", duration);
    
    println!("Parsed {} records from NMEA sentences.", records.len());

    record_db.insert_many(records);

    println!("Database now contains {} records, with size {} KB", record_db.len(), record_db.check_memory()/(1024));

let start = std::time::Instant::now();
    let mut arcs = gnssir::find_arcs(record_db.records.clone());
let duration: std::time::Duration = start.elapsed();
    println!("Finding arcs took: {:?}", duration);

    // let arc0_records_indices = &arcs[1].record_indices;
    // let arc0_records: Vec<&db::record::Record> = arc0_records_indices.iter().map(|&i| &record_db.records[i]).collect();
    // let mut wtr = Writer::from_path("arc_0_pre.csv").expect("Failed to create CSV file");
    // for record in arc0_records {
    //     wtr.write_record(&[
    //         record.id.to_string(),
    //         record.snr.to_string(),
    //         record.elevation.to_string(),
    //         record.azimuth.to_string(),
    //         record.time.to_string(),
    //     ]).expect("Failed to write record");
    // }
    // wtr.flush().expect("Failed to flush CSV writer");

let start = std::time::Instant::now();
        
    // let mut wtr = Writer::from_path(format!("arc_freqs.csv")).expect("Failed to create CSV file");
    // wtr.write_record(&["id", "frequency", "amplitude"]).expect("Failed to write header");

    for arc in &arcs {
        gnssir::fix_arc_elev_azim(arc, &mut record_db.records);
        gnssir::correct_arc_snr(arc, &mut record_db.records);

        // for (freq, amp) in results {
        //     wtr.write_record(&[arc.sat_id.to_string(), freq.to_string(), amp.to_string()]).expect("Failed to write record");
        // }
    }
    // wtr.flush().expect("Failed to flush CSV writer");
let duration = start.elapsed();
    println!("Data correction took: {:?}", duration);

let start = std::time::Instant::now();
    for arc in &arcs {
        let results = gnssir::find_arc_frequencies(arc, &record_db.records);
        println!("Arc ID {}: Found {} frequency components", arc.sat_id, results.len());
    }
let duration = start.elapsed();
    println!("Frequency analysis took: {:?}", duration);

    // let arc0_records_corrected: Vec<&db::record::Record> = arc0_records_indices.iter().map(|&i| &record_db.records[i]).collect();
    // let mut wtr = Writer::from_path("arc_0_post.csv").expect("Failed to create CSV file");
    // for record in arc0_records_corrected {
    //     wtr.write_record(&[
    //         record.id.to_string(),
    //         record.snr.to_string(),
    //         record.elevation.to_string(),
    //         record.azimuth.to_string(),
    //         record.time.to_string(),
    //     ]).expect("Failed to write record");
    // }
    // wtr.flush().expect("Failed to flush CSV writer");
}
