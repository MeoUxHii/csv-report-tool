use chrono::{Datelike, Duration, NaiveDate};
use csv::{ReaderBuilder, WriterBuilder};
use rfd::{FileDialog, MessageDialog, MessageLevel};
use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct GroupKey {
    report_date: String,
    report_week: String,
    report_month: String,
    sub_channel: String,
    main_channel: String,
    qa_handle: String,
    sen_handle: String,
    agent_email: String,
    seniority: String,
    level1_case_category_name: String,
    level3_case_category_name: String,
    shop_id: String,
}

#[derive(Default, Debug)]
struct Counts {
    good_cnt: u64,
    bad_cnt: u64,
    good_bad_cnt: u64,
    rating_cnt: u64,
    contact_cnt: u64,
}

fn clean_header(h: &str) -> String {
    h.trim_matches(|c: char| c == '\u{FEFF}' || c == '"' || c.is_whitespace())
        .to_string()
}

fn parse_grass_date(raw: &str) -> Option<NaiveDate> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(s, "%d/%m/%Y")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .or_else(|_| NaiveDate::parse_from_str(s, "%d-%m-%Y"))
        .ok()
}

fn map_sub_channel(shop_tag_name: &str) -> String {
    match shop_tag_name.trim() {
        "CS_AgentRouting_AgentReply_LocalShop_VN_SBS_Live" => "SBS".to_string(),
        "CS_AgentRouting_AgentReply_LocalShop_VN_KRCB_Live" => "CBKR".to_string(),
        "CS_AgentRouting_AgentReply_LocalShop_VN_Lovito_Live" | "Lovito_VN" | "" => {
            "CBCN".to_string()
        }
        "CS_AgentRouting_AgentReply_LocalShop_VN_SCS_Live" => "SCS".to_string(),
        other => other.to_string(),
    }
}

fn map_main_channel(sub_channel: &str) -> String {
    match sub_channel {
        "SBS" | "SCS" => "SBS".to_string(),
        "CBCN" | "CBKR" | "CBKT" => "CBCS".to_string(),
        other => other.to_string(),
    }
}

fn process_csv(input_path: &Path, output_path: &Path) -> Result<(usize, usize), String> {
    let file = File::open(input_path).map_err(|e| format!("Không thể mở file input: {e}"))?;
    let mut rdr = ReaderBuilder::new().has_headers(true).from_reader(file);

    let raw_headers = rdr
        .headers()
        .map_err(|e| format!("Lỗi đọc header CSV: {e}"))?
        .clone();

    let find_col = |name: &str| -> Result<usize, String> {
        raw_headers
            .iter()
            .position(|h| clean_header(h) == name)
            .ok_or_else(|| format!("Không tìm thấy cột bắt buộc '{name}' trong file CSV."))
    };

    let idx_grass_date = find_col("grass_date")?;
    let idx_shop_tag = find_col("shop_tag_name")?;
    let idx_agent_email = find_col("agent_email")?;
    let idx_level1 = find_col("level1_case_category_name")?;
    let idx_level3 = find_col("level3_case_category_name")?;
    let idx_shop_id = find_col("shop_id")?;
    let idx_csat_level = find_col("csat_level")?;

    let mut aggregated: BTreeMap<GroupKey, Counts> = BTreeMap::new();
    let mut total_rows = 0usize;

    for (row_idx, result) in rdr.records().enumerate() {
        let record = result.map_err(|e| format!("Lỗi đọc dòng {}: {e}", row_idx + 2))?;
        let raw_date = record.get(idx_grass_date).unwrap_or("").trim();
        if raw_date.is_empty() {
            continue;
        }

        let date = parse_grass_date(raw_date).ok_or_else(|| {
            format!(
                "Không thể đọc ngày '{}' ở dòng {} (cần định dạng DD/MM/YYYY).",
                raw_date,
                row_idx + 2
            )
        })?;

        total_rows += 1;

        let report_date = date.format("%d/%m/%Y").to_string();
        let days_from_mon = date.weekday().num_days_from_monday() as i64;
        let monday = date - Duration::days(days_from_mon);
        let report_week = monday.format("%d/%m/%Y").to_string();
        let first_of_month = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
            .ok_or_else(|| "Lỗi tính ngày đầu tháng".to_string())?;
        let report_month = first_of_month.format("%d/%m/%Y").to_string();

        let shop_tag_name = record.get(idx_shop_tag).unwrap_or("");
        let sub_channel = map_sub_channel(shop_tag_name);
        let main_channel = map_main_channel(&sub_channel);

        let agent_email = record.get(idx_agent_email).unwrap_or("").trim().to_string();

        let l1_raw = record.get(idx_level1).unwrap_or("").trim();
        let level1 = if l1_raw.is_empty() {
            "-".to_string()
        } else {
            l1_raw.to_string()
        };

        let l3_raw = record.get(idx_level3).unwrap_or("").trim();
        let level3 = if l3_raw.is_empty() {
            "-".to_string()
        } else {
            l3_raw.to_string()
        };

        let shop_id = record.get(idx_shop_id).unwrap_or("").trim().to_string();
        let csat_level = record.get(idx_csat_level).unwrap_or("").trim();

        let key = GroupKey {
            report_date,
            report_week,
            report_month,
            sub_channel,
            main_channel,
            qa_handle: String::new(),
            sen_handle: String::new(),
            agent_email,
            seniority: String::new(),
            level1_case_category_name: level1,
            level3_case_category_name: level3,
            shop_id,
        };

        let entry = aggregated.entry(key).or_default();
        match csat_level {
            "1" => {
                entry.good_cnt += 1;
                entry.good_bad_cnt += 1;
                entry.rating_cnt += 1;
                entry.contact_cnt += 1;
            }
            "2" => {
                entry.rating_cnt += 1;
                entry.contact_cnt += 1;
            }
            "3" => {
                entry.bad_cnt += 1;
                entry.good_bad_cnt += 1;
                entry.rating_cnt += 1;
                entry.contact_cnt += 1;
            }
            "0" => {
                entry.contact_cnt += 1;
            }
            _ => {}
        }
    }

    let out_file =
        File::create(output_path).map_err(|e| format!("Không thể tạo file output: {e}"))?;
    let mut wtr = WriterBuilder::new().from_writer(out_file);

    wtr.write_record([
        "report_date",
        "report_week",
        "report_month",
        "sub_channel",
        "main_channel",
        "qa_handle",
        "sen_handle",
        "agent_email",
        "seniority",
        "level1_case_category_name",
        "level3_case_category_name",
        "shop_id",
        "good_cnt",
        "bad_cnt",
        "good_bad_cnt",
        "rating_cnt",
        "contact_cnt",
    ])
    .map_err(|e| format!("Lỗi ghi header output: {e}"))?;

    let output_rows = aggregated.len();
    for (k, c) in aggregated {
        wtr.write_record([
            k.report_date,
            k.report_week,
            k.report_month,
            k.sub_channel,
            k.main_channel,
            k.qa_handle,
            k.sen_handle,
            k.agent_email,
            k.seniority,
            k.level1_case_category_name,
            k.level3_case_category_name,
            k.shop_id,
            c.good_cnt.to_string(),
            c.bad_cnt.to_string(),
            c.good_bad_cnt.to_string(),
            c.rating_cnt.to_string(),
            c.contact_cnt.to_string(),
        ])
        .map_err(|e| format!("Lỗi ghi dòng output: {e}"))?;
    }

    wtr.flush()
        .map_err(|e| format!("Lỗi lưu file output: {e}"))?;

    Ok((total_rows, output_rows))
}

fn default_output_path(input: &Path) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    parent.join(format!("{stem}_output.csv"))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (input_path, output_path) = if args.len() >= 2 {
        let in_p = PathBuf::from(&args[1]);
        let out_p = if args.len() >= 3 {
            PathBuf::from(&args[2])
        } else {
            default_output_path(&in_p)
        };
        (in_p, out_p)
    } else {
        let Some(in_p) = FileDialog::new()
            .set_title("Chọn file CSV đầu vào")
            .add_filter("CSV Files", &["csv"])
            .pick_file()
        else {
            return;
        };

        let suggested_name = default_output_path(&in_p);
        let file_name = suggested_name
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("output.csv");

        let mut save_dialog = FileDialog::new()
            .set_title("Chọn nơi lưu file CSV kết quả")
            .add_filter("CSV Files", &["csv"])
            .set_file_name(file_name);

        if let Some(dir) = in_p.parent() {
            save_dialog = save_dialog.set_directory(dir);
        }

        let Some(out_p) = save_dialog.save_file() else {
            return;
        };

        (in_p, out_p)
    };

    match process_csv(&input_path, &output_path) {
        Ok((in_cnt, out_cnt)) => {
            let msg = format!(
                "Chuyển đổi thành công!\n\n- Số dòng đọc từ input: {in_cnt}\n- Số dòng sau khi gom nhóm (output): {out_cnt}\n- Đã lưu tại:\n{}",
                output_path.display()
            );
            println!("{msg}");
            if args.len() < 2 {
                MessageDialog::new()
                    .set_level(MessageLevel::Info)
                    .set_title("Hoàn tất")
                    .set_description(&msg)
                    .show();
            }
        }
        Err(err) => {
            eprintln!("Lỗi: {err}");
            if args.len() < 2 {
                MessageDialog::new()
                    .set_level(MessageLevel::Error)
                    .set_title("Lỗi xử lý CSV")
                    .set_description(&err)
                    .show();
            }
            std::process::exit(1);
        }
    }
}