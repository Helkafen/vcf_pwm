extern crate bio;
extern crate bgzip;
extern crate clap;

use std::rc::Rc;
use rust_htslib::bcf::*;

use bgzip::write::BGzWriter;
use std::fs;
use std::io::prelude::*;

use std::collections::HashMap;
use std::collections::HashSet;

use std::cmp::min;
use std::path::Path;
use std::time::SystemTime;

use clap::{Arg, App};

mod types;
mod range;
mod pattern;
mod util;
mod haplotype;
mod bed;

use types::*;
use range::Range;
use pattern::{parse_pwm_files,matches};
use util::to_nucleotides_pos;
use haplotype::load_haplotypes;
use bed::load_peak_files;


//fn run_matches(i: u64) {
//    let c = Weight::new(0, 1000, 0, 0);
//    let g = Weight::new(0, 0, 1000, 0);
//    let pwm = PWM {weights: vec![c,g], name: "pwm".to_string(), pattern_id: 5, min_score: 1500};
//    let haplotype = vec![
//        NucleotidePos { nuc: Nucleotide::A, pos: 10 },
//        NucleotidePos { nuc: Nucleotide::C, pos: 11 },
//        NucleotidePos { nuc: Nucleotide::G, pos: 12 },
//        NucleotidePos { nuc: Nucleotide::T, pos: i }
//    ];
//    let haplotype_ids = Rc::new(Vec::new());
//    let m = matches(&pwm, &haplotype, haplotype_ids.clone());
//    let expected = vec![Match {pos: 11, pattern_id: 5, haplotype_ids: haplotype_ids.clone()}];
//    if( i + m.len() as u64) % 10000000 == 0 {
//        println!("{} {}",i, m.len());
//    }
//    assert_eq!(m, expected);
//}

fn get_sample_names(reader: &IndexedReader) -> Vec<String> {
    let mut x = Vec::new();
    for i in reader.header().samples().iter() {
        x.push(String::from_utf8_lossy(i).into_owned());
    }
    x
}

fn repeat(size: usize, val: u32) -> Vec<u32> {
    let mut x = Vec::new();
    for _i in 0..size {
        x.push(val);
    }
    x
}

fn select_inner_peaks(peak: Range, peak_map: &HashMap<String, Vec<Range>>) -> HashMap<&String, Vec<&Range>> {
    let mut ip = HashMap::new();
    for (s,ps) in peak_map.iter() {
        for p in ps {
            if p.overlaps(&peak) {
                ip.entry(s).or_insert(Vec::new()).push(p)
            }
        }
    }
    ip
}


// cargo run --release -- -c chr1 -i /home/seb/masters/topmed/source/TOPMed_dbGaP_20180710/dbGaP-12336/65066/topmed-dcc/exchange/phs000964_TOPMed_WGS_JHS/Combined_Study_Data/Genotypes/freeze.6a/phased/freeze.6a.chr1.pass_only.phased.bcf -b Bcell-13,CD4-9,CD8-10,CLP-14,CMP-4,Erythro-15,GMP-5,HSC-1,LMPP-3,MCP,mDC,MEGA1,MEGA2,MEP-6,Mono-7,MPP-2,Nkcell-11,pDC -o test2.gz -p /home/seb/masters/regu/dnamotifs/HOCOMOCOv11_full_pwms_HUMAN_mono.txt -n JUNB_HUMAN.H11MO.0.A,FOSL1_HUMAN.H11MO.0.A,FOSL2_HUMAN.H11MO.0.A,JDP2_HUMAN.H11MO.0.D,GATA1_HUMAN.H11MO.0.A,GATA2_HUMAN.H11MO.0.A,GATA3_HUMAN.H11MO.0.A,GATA4_HUMAN.H11MO.0.A,GATA5_HUMAN.H11MO.0.D,GATA6_HUMAN.H11MO.0.A,JUN_HUMAN.H11MO.0.A,JUND_HUMAN.H11MO.0.A,BATF_HUMAN.H11MO.0.A,ATF3_HUMAN.H11MO.0.A,BACH1_HUMAN.H11MO.0.A,BACH2_HUMAN.H11MO.0.A,NFE2_HUMAN.H11MO.0.A,CEBPA_HUMAN.H11MO.0.A,CEBPB_HUMAN.H11MO.0.A,CEBPD_HUMAN.H11MO.0.C,CEBPE_HUMAN.H11MO.0.A,CEBPG_HUMAN.H11MO.0.B,SPIB_HUMAN.H11MO.0.A,IRF8_HUMAN.H11MO.0.B,SPI1_HUMAN.H11MO.0.A,MESP1_HUMAN.H11MO.0.D,ID4_HUMAN.H11MO.0.D,HTF4_HUMAN.H11MO.0.A,ITF2_HUMAN.H11MO.0.C,STAT1_HUMAN.H11MO.0.A,STAT2_HUMAN.H11MO.0.A,SPIC_HUMAN.H11MO.0.D,CTCF_HUMAN.H11MO.0.A,IRF1_HUMAN.H11MO.0.A,DBP_HUMAN.H11MO.0.B,MAFK_HUMAN.H11MO.1.A,ATF4_HUMAN.H11MO.0.A,ASCL1_HUMAN.H11MO.0.A,ASCL2_HUMAN.H11MO.0.D,TFE2_HUMAN.H11MO.0.A,MYOD1_HUMAN.H11MO.0.A,EVI1_HUMAN.H11MO.0.B,IRF3_HUMAN.H11MO.0.B,ZEB1_HUMAN.H11MO.0.A,IRF9_HUMAN.H11MO.0.C,HEN1_HUMAN.H11MO.0.C,LYL1_HUMAN.H11MO.0.A -t /home/seb/masters/regu/dnamotifs/hocomoco_thresholds.tab -r /home/seb/masters/hg38.fa

fn main() {
    let opt_matches = App::new("VCF_PWM")
                        .version("1.0")
                        .author("Sébastian Méric de Bellefon <arnaudpourseb@gmail.com>")
                        .about("Find patterns in a VCF file")
                        .arg(Arg::with_name("chromosome")        .short("c").required(true).takes_value(true).value_name("CHROM")    .long("chromosome")        .help("Chromosome to scan. Ex: 'chr1'"))
                        .arg(Arg::with_name("bcf")               .short("i").required(true).takes_value(true).value_name("IN")       .long("input")             .help("BCF input file to use"))
                        .arg(Arg::with_name("output")            .short("o").required(true).takes_value(true).value_name("OUT")      .long("output")            .help("Output VCF file"))
                        .arg(Arg::with_name("ref")               .short("r").required(true).takes_value(true).value_name("REF")      .long("reference")         .help("Reference genome. Ex: hg38.fa"))
                        .arg(Arg::with_name("bed_files")         .short("b").required(true).takes_value(true).value_name("BED")      .long("bed")               .help("Bed files containing the regions to scan"))
                        .arg(Arg::with_name("pwm_names")         .short("n").required(true).takes_value(true).value_name("PWM_NAMES").long("pwm_names")         .help("List of PWM names to scan"))
                        .arg(Arg::with_name("pwm_file")          .short("p").required(true).takes_value(true).value_name("PWM")      .long("pwm_file")          .help("PWM file. Ex: HOCOMOCOv11_full_pwms_HUMAN_mono.txt"))
                        .arg(Arg::with_name("pwm_threshold_file").short("t").required(true).takes_value(true).value_name("THRESHOLD").long("pwm_threshold_file").help("PWM threshold file. Ex: HOCOMOCOv11_full_standard_thresholds_HUMAN_mono.txt"))
                        .get_matches();

    let chromosome               = opt_matches.value_of("chromosome").unwrap();                     //chr1
    let bcf                      = opt_matches.value_of("bcf").unwrap();                            //format!("/home/seb/masters/topmed/source/TOPMed_dbGaP_20180710/dbGaP-12336/65066/topmed-dcc/exchange/phs000964_TOPMed_WGS_JHS/Combined_Study_Data/Genotypes/freeze.6a/phased/freeze.6a.{}.pass_only.phased.bcf", chromosome);
    let bed_files: Vec<&str>     = opt_matches.value_of("bed_files").unwrap().split(',').collect(); //"Bcell-13,CD4-9,CD8-10,CLP-14,CMP-4,Erythro-15,GMP-5,HSC-1,LMPP-3,MCP,mDC,MEGA1,MEGA2,MEP-6,Mono-7,MPP-2,Nkcell-11,pDC".split(',').collect();
    let reference_genome_file    = opt_matches.value_of("ref").unwrap();                            //"/home/seb/masters/hg38.fa";
    let pwm_file                 = opt_matches.value_of("pwm_file").unwrap();                       //"/home/seb/masters/regu/dnamotifs/HOCOMOCOv11_full_pwms_HUMAN_mono.txt";
    let pwm_threshold_file       = opt_matches.value_of("pwm_threshold_file").unwrap();             //"/home/seb/masters/regu/dnamotifs/hocomoco_thresholds.tab";
    let wanted_pwms: Vec<String> = opt_matches.value_of("pwm_names").unwrap().split(',').map(|s| s.to_string()).collect(); //"JUNB_HUMAN.H11MO.0.A,FOSL1_HUMAN.H11MO.0.A,FOSL2_HUMAN.H11MO.0.A,JDP2_HUMAN.H11MO.0.D,GATA1_HUMAN.H11MO.0.A,GATA2_HUMAN.H11MO.0.A,GATA3_HUMAN.H11MO.0.A,GATA4_HUMAN.H11MO.0.A,GATA5_HUMAN.H11MO.0.D,GATA6_HUMAN.H11MO.0.A,JUN_HUMAN.H11MO.0.A,JUND_HUMAN.H11MO.0.A,BATF_HUMAN.H11MO.0.A,ATF3_HUMAN.H11MO.0.A,BACH1_HUMAN.H11MO.0.A,BACH2_HUMAN.H11MO.0.A,NFE2_HUMAN.H11MO.0.A,CEBPA_HUMAN.H11MO.0.A,CEBPB_HUMAN.H11MO.0.A,CEBPD_HUMAN.H11MO.0.C,CEBPE_HUMAN.H11MO.0.A,CEBPG_HUMAN.H11MO.0.B,SPIB_HUMAN.H11MO.0.A,IRF8_HUMAN.H11MO.0.B,SPI1_HUMAN.H11MO.0.A,MESP1_HUMAN.H11MO.0.D,ID4_HUMAN.H11MO.0.D,HTF4_HUMAN.H11MO.0.A,ITF2_HUMAN.H11MO.0.C,STAT1_HUMAN.H11MO.0.A,STAT2_HUMAN.H11MO.0.A,SPIC_HUMAN.H11MO.0.D,CTCF_HUMAN.H11MO.0.A,IRF1_HUMAN.H11MO.0.A,DBP_HUMAN.H11MO.0.B,MAFK_HUMAN.H11MO.1.A,ATF4_HUMAN.H11MO.0.A,ASCL1_HUMAN.H11MO.0.A,ASCL2_HUMAN.H11MO.0.D,TFE2_HUMAN.H11MO.0.A,MYOD1_HUMAN.H11MO.0.A,EVI1_HUMAN.H11MO.0.B,IRF3_HUMAN.H11MO.0.B,ZEB1_HUMAN.H11MO.0.A,IRF9_HUMAN.H11MO.0.C,HEN1_HUMAN.H11MO.0.C,LYL1_HUMAN.H11MO.0.A".split(',').into_iter().map(|a| a.to_string()).collect();
    let output_file              = opt_matches.value_of("output").unwrap();                         //"test2.gz";


    let pwm_list: Vec<PWM> = parse_pwm_files(pwm_file, pwm_threshold_file).iter().filter(|p| wanted_pwms.contains(&p.name)).cloned().collect();
    for pwm in &pwm_list {
        println!("PWM {} {}", pwm.name, pwm.min_score);
    }

    let (merged_peaks, peak_map) = load_peak_files(&bed_files, chromosome);

    let mut reader = IndexedReader::from_path(bcf).expect("Error while opening the bcf file");
    let mut reference_genome = bio::io::fasta::IndexedReader::from_file(&Path::new(reference_genome_file)).expect("Error while opening the reference genome");
    let mut writer = BGzWriter::new(fs::File::create(output_file).expect("Could not create output file"));

    let rid = reader.header().name2rid(chromosome.as_bytes()).unwrap();
    let samples = get_sample_names(&mut reader);
    let sample_count = samples.len();
    let null_count: Vec<u32> = repeat(sample_count, 0);
    println!("Number of samples: {}", sample_count);
    let mut peak_id = 0;
    let number_of_peaks = &merged_peaks.len();
    let all_haplotypes_with_reference_genome: HashSet<HaplotypeId> = {
            let mut x = HashSet::new();
            for i in (0..sample_count).into_iter() {
                x.insert(HaplotypeId {sample_id: i, side: HaplotypeSide::Left});
                x.insert(HaplotypeId {sample_id: i, side: HaplotypeSide::Right});
            }
            x
        };

    // Write header in output file
    writer.write("#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT".as_bytes()).expect("Could not create output file");
    for sample in samples {
        writer.write("\t".as_bytes()).expect("Could not create output file");
        writer.write(sample.as_bytes()).expect("Could not create output file");
    }writer.write("\n".as_bytes()).expect("Could not create output file");

    // A fake and unique position in the chromosome given for each line in the resulting vcf
    let mut fake_position: u32 = 1;

    let start_time = SystemTime::now();

    for peak in merged_peaks {
        let peak_start_time = SystemTime::now();
        peak_id = peak_id + 1;
        reference_genome.fetch(chromosome, peak.start, peak.end - 1).expect("Error while seeking in reference genome file");
        let ref_genome_peak: Vec<NucleotidePos> = {
            let mut text = Vec::new();
            reference_genome.read(&mut text).expect("Error while reading in reference genome file");
            to_nucleotides_pos(&text, &peak)
        };
        let ref_genome = |r: Range| -> Vec<NucleotidePos> {
            ref_genome_peak.iter().cloned().filter(|n| n.pos >= r.start && n.pos <= r.end).collect()
        };

        let inner_peaks: HashMap<&String, Vec<&Range>> = select_inner_peaks(peak, &peak_map);
        reader.fetch(rid, peak.start as u32, peak.end as u32).unwrap();

        let mut match_list = Vec::new();
        let mut number_of_haplotypes = 0;
        let mut haplotypes_with_reference_genome: HashSet<HaplotypeId> = all_haplotypes_with_reference_genome.iter().cloned().collect();

        let (variant_count, mut xs) = load_haplotypes(chromosome, &peak, &mut reader, ref_genome);
        for (haplotype, haplotype_ids) in xs.drain() {
            //for d in &diffs {
            //    println!("{} {:?} {:?}", d.pos, d.reference, d.alternative);
            //}
            //println!("In group_by_diffs iter");
            for h in haplotype_ids.iter()  {
                haplotypes_with_reference_genome.remove(&h);
            }
            for pwm in &pwm_list {
                match_list.extend(matches(pwm, &haplotype, haplotype_ids.clone()));
            }
            number_of_haplotypes = number_of_haplotypes + 1;
        };
        if !haplotypes_with_reference_genome.is_empty() {
            number_of_haplotypes = number_of_haplotypes + 1;
            let haplotype = ref_genome(peak.clone());
            let x: Vec<HaplotypeId> = haplotypes_with_reference_genome.into_iter().collect();
            let hap_ids = Rc::new(x);
            for pwm in &pwm_list {
                match_list.extend(matches(pwm, &haplotype, hap_ids.clone()));
            }
        }

        let mut counts = count_matches_by_sample(&match_list, &inner_peaks, &null_count);


        for ((source, inner_peak, pattern_id),v) in counts.drain() {
            let (distinct_counts, genotypes) = counts_as_genotypes(v);

            let id_str = format!("{},{},{}-{}",source, pattern_id, inner_peak.start, inner_peak.end);
            let distinct_counts_str: Vec<String> = distinct_counts.iter().map(|c| c.to_string()).collect();
            let info_str = format!("COUNTS={}", distinct_counts_str.join(",")); // <> T.intercalate "," (map showt (Set.toList s))
            writer.write(format!("{}\t{}\t{}\t.\t.\t.\t.\t{}", chromosome, fake_position, id_str, info_str).as_bytes()).expect("Could not write result");

            writer.write(genotypes.as_bytes()).expect("Could not write result");
            writer.write("\n".as_bytes()).expect("Could not write result");
            fake_position = fake_position + 1;
        }



        let number_of_matches: u64 = match_list.iter().map(|m| m.haplotype_ids.len() as u64).sum();
        let peak_time_elapsed = peak_start_time.elapsed().unwrap().as_millis();
        let global_time_elapsed = start_time.elapsed().unwrap().as_millis();
        println!("Peak {}/{}\t{} ms ({} total)\t{}\t{}\t{} haplotypes\t{} variants\t{} matches", peak_id, number_of_peaks, peak_time_elapsed, global_time_elapsed, peak.start, peak.end, number_of_haplotypes, variant_count, number_of_matches);
    }
}

fn counts_as_genotypes(v: Vec<u32>) -> (Vec<u32>, String) {
    let mut res = String::with_capacity(v.len()*4);
    let min = v.iter().min();
    let max = v.iter().max();
    match (min, max) {
        (Some(&lowest), Some(&highest)) => {
            let intermediate_1_1000 = (lowest * 1000 * 3 + highest * 1000    ) / 4;
            let intermediate_3_1000 = (lowest * 1000     + highest * 1000 * 3) / 4;
            let mut all_values = vec![lowest, highest];
            for x in v {
                if x == lowest {res.push_str("\t0|0");}
                else if x == highest {res.push_str("\t1|1");}
                else {
                    if !all_values.contains(&x) { all_values.push(x);}
                    let x_1000 = x *1000;
                    if x_1000 < intermediate_1_1000 { res.push_str("\t0|0"); }
                    else if x_1000 < intermediate_3_1000 { res.push_str("\t0|1"); }
                    else { res.push_str("\t1|1"); }
                }
            }
            all_values.sort();
            (all_values, res)
        },
        (None, _) => (Vec::new(), String::new()),
        (_, None) => (Vec::new(), String::new()),
    }
}

fn count_matches_by_sample<'a>(match_list: &Vec<Match>, inner_peaks: &'a HashMap<&String, Vec<&Range>>, null_count: &Vec<u32>) -> HashMap<(&'a String, &'a Range, u16), Vec<u32>> {
    let mut pppp: HashMap<(&String, &Range, u16), Vec<u32>> = HashMap::new();
    //println!("count_matches_by_sample: {} match objects, {} inner_peak", match_list.len(), inner_peaks.len());
    for m in match_list {
        let pos = m.pos;
        for (&s,inner) in inner_peaks.iter().map(|(s,x)| (s, x.iter().filter(|y| y.contains(pos)))) {
            for &inner_peak in inner {
                let key = (s, inner_peak, m.pattern_id);
                match pppp.get_mut(&key) {
                    Some(x) => {
                        for haplotype_id in m.haplotype_ids.iter() {
                            x[haplotype_id.sample_id as usize] = x[haplotype_id.sample_id as usize] + 1;
                        }
                    }
                    None => {
                        let mut x = null_count.clone();
                        for haplotype_id in m.haplotype_ids.iter() {
                            x[haplotype_id.sample_id as usize] = x[haplotype_id.sample_id as usize] + 1;
                        }
                        pppp.insert(key, x);
                    }
                }
            }
        }
    }
    pppp
}


#[cfg(test)]
mod tests {
    use super::*;
    use haplotype::patch_haplotype;

    fn ref_genome(r: Range) -> Vec<NucleotidePos> {
        let chromosome = vec![
            NucleotidePos { nuc: Nucleotide::A, pos: 0 },
            NucleotidePos { nuc: Nucleotide::C, pos: 1 },
            NucleotidePos { nuc: Nucleotide::G, pos: 2 },
            NucleotidePos { nuc: Nucleotide::T, pos: 3 }
            ];
        return chromosome[r.start as usize .. min((r.end+1) as usize,chromosome.len())].to_vec();
    }

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_matches() {
        let c = Weight::new(0, 1000, 0, 0);
        let g = Weight::new(0, 0, 1000, 0);
        let pwm = PWM {weights: vec![c,g], name: "pwm".to_string(), pattern_id: 5, min_score: 1500};
        let haplotype = vec![
            NucleotidePos { nuc: Nucleotide::A, pos: 10 },
            NucleotidePos { nuc: Nucleotide::C, pos: 11 },
            NucleotidePos { nuc: Nucleotide::G, pos: 12 },
            NucleotidePos { nuc: Nucleotide::T, pos: 13 }
        ];
        let haplotype_ids = Rc::new(Vec::new());
        let m = matches(&pwm, &haplotype, haplotype_ids.clone());
        let expected = vec![Match {pos: 11, pattern_id: 5, haplotype_ids: haplotype_ids.clone()}];
        assert_eq!(m, expected);
    }

    #[test]
    fn test_patch_haplotype_with_no_diff() {
        let diffs = Vec::new();
        let patched = patch_haplotype(&Range::new(1,2), &diffs, &ref_genome);
        let expected = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched, expected);

        let patched2 = patch_haplotype(&Range::new(0,2), &diffs, &ref_genome);
        let expected2 = vec![NucleotidePos { nuc: Nucleotide::A, pos: 0 }, NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched2, expected2);

        let patched3 = patch_haplotype(&Range::new(0,5), &diffs, &ref_genome);
        let expected3 = vec![NucleotidePos { nuc: Nucleotide::A, pos: 0 }, NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }, NucleotidePos { nuc: Nucleotide::T, pos: 3 }];
        assert_eq!(patched3, expected3);
    }

    #[test]
    fn test_patch_haplotype_one_snp() {
        let diffs = vec![Diff { pos: 100, reference: vec![Nucleotide::A], alternative: vec![Nucleotide::C] }];
        let patched = patch_haplotype(&Range::new(1,2), &diffs, &ref_genome);
        let expected = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched, expected);

        let diffs2 = vec![Diff { pos: 1, reference: vec![Nucleotide::C], alternative: vec![Nucleotide::N] }];
        let patched2 = patch_haplotype(&Range::new(1,2), &diffs2, &ref_genome);
        let expected2 = vec![NucleotidePos { nuc: Nucleotide::N, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched2, expected2);

        let diffs3 = vec![Diff { pos: 2, reference: vec![Nucleotide::G], alternative: vec![Nucleotide::A] }];
        let patched3 = patch_haplotype(&Range::new(1,2), &diffs3, &ref_genome);
        let expected3 = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::A, pos: 2 }];
        assert_eq!(patched3, expected3);
    }

    #[test]
    fn test_patch_haplotype_two_snp() {
        let diffs = vec![Diff { pos: 1, reference: vec![Nucleotide::C], alternative: vec![Nucleotide::N] }, Diff { pos: 2, reference: vec![Nucleotide::G], alternative: vec![Nucleotide::A] }];
        let patched = patch_haplotype(&Range::new(1,2), &diffs, &ref_genome);
        let expected = vec![NucleotidePos { nuc: Nucleotide::N, pos: 1 }, NucleotidePos { nuc: Nucleotide::A, pos: 2 }];
        assert_eq!(patched, expected);

        let diffs2 = vec![Diff { pos: 1, reference: vec![Nucleotide::C], alternative: vec![Nucleotide::N] }, Diff { pos: 4, reference: vec![Nucleotide::G], alternative: vec![Nucleotide::A] }];
        let patched2 = patch_haplotype(&Range::new(1,2), &diffs2, &ref_genome);
        let expected2 = vec![NucleotidePos { nuc: Nucleotide::N, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched2, expected2);
    }

    #[test]
    fn test_patch_haplotype_one_insert() {
        let diffs = vec![Diff { pos: 1, reference: vec![Nucleotide::C], alternative: vec![Nucleotide::N, Nucleotide::N] }];
        let patched = patch_haplotype(&Range::new(1,2), &diffs, &ref_genome);
        let expected = vec![NucleotidePos { nuc: Nucleotide::N, pos: 1 }, NucleotidePos { nuc: Nucleotide::N, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched, expected);

        let diffs2 = vec![Diff { pos: 2, reference: vec![Nucleotide::C], alternative: vec![Nucleotide::N, Nucleotide::N] }];
        let patched2 = patch_haplotype(&Range::new(1,2), &diffs2, &ref_genome);
        let expected2 = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::N, pos: 2 }, NucleotidePos { nuc: Nucleotide::N, pos: 2 }];
        assert_eq!(patched2, expected2);

        let diffs3 = vec![Diff { pos: 3, reference: vec![Nucleotide::C], alternative: vec![Nucleotide::N, Nucleotide::N] }];
        let patched3 = patch_haplotype(&Range::new(1,2), &diffs3, &ref_genome);
        let expected3 = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched3, expected3);
    }

    #[test]
    fn test_patch_haplotype_one_deletion() {
        let diffs = vec![Diff { pos: 1, reference: vec![Nucleotide::C, Nucleotide::G], alternative: vec![Nucleotide::C] }];
        let patched = patch_haplotype(&Range::new(1,2), &diffs, &ref_genome);
        let expected = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }];
        assert_eq!(patched, expected);

        let diffs2 = vec![Diff { pos: 2, reference: vec![Nucleotide::G, Nucleotide::T], alternative: vec![Nucleotide::G] }];
        let patched2 = patch_haplotype(&Range::new(1,2), &diffs2, &ref_genome);
        let expected2 = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched2, expected2);

        // For simplicity, do not apply a Diff that starts before the window we're observing
        let diffs3 = vec![Diff { pos: 0, reference: vec![Nucleotide::A, Nucleotide::C], alternative: vec![Nucleotide::A] }];
        let patched3 = patch_haplotype(&Range::new(1,2), &diffs3, &ref_genome);
        let expected3 = vec![NucleotidePos { nuc: Nucleotide::C, pos: 1 }, NucleotidePos { nuc: Nucleotide::G, pos: 2 }];
        assert_eq!(patched3, expected3);
    }
}