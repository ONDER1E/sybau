use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::Duration;
use chrono::{Datelike, Timelike, Utc};
use serde::Deserialize;

// -- SoftClock Thread (Fallback Time) --

#[derive(Debug, Clone, Copy)]
struct SoftClock {
    sec: u8,
    min: u8,
    hour: u8,
    day: u8,
    month: u8,
    year: u16,
}

impl SoftClock {
    fn from_system_time() -> Self {
        let now = Utc::now();
        Self {
            sec: now.second() as u8,
            min: now.minute() as u8,
            hour: now.hour() as u8,
            day: now.day() as u8,
            month: now.month() as u8,
            year: now.year() as u16,
        }
    }

    fn tick(&mut self) {
        self.sec += 1;
        if self.sec >= 60 {
            self.sec = 0;
            self.min += 1;
            if self.min >= 60 {
                self.min = 0;
                self.hour += 1;
                if self.hour >= 24 {
                    self.hour = 0;
                    self.day += 1;
                    if self.day > 31 {
                        self.day = 1;
                        self.month += 1;
                        if self.month > 12 {
                            self.month = 1;
                            self.year += 1;
                        }
                    }
                }
            }
        }
    }
}

struct ClockHandle {
    clock: Arc<Mutex<SoftClock>>,
    ready: Arc<AtomicBool>,
}

impl ClockHandle {
    fn start() -> Self {
        let clock = Arc::new(Mutex::new(SoftClock::from_system_time()));
        let ready = Arc::new(AtomicBool::new(false));

        let clock_clone = Arc::clone(&clock);
        let ready_clone = Arc::clone(&ready);

        thread::spawn(move || {
            ready_clone.store(true, Ordering::SeqCst);
            loop {
                thread::sleep(Duration::from_secs(1));
                let mut locked = clock_clone.lock().unwrap();
                locked.tick();
            }
        });

        Self { clock, ready }
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    fn get_time(&self) -> SoftClock {
        self.clock.lock().unwrap().clone()
    }
}

// -- Online Time Source Fetching --

#[derive(Debug, Deserialize)]
struct WorldTimeApiResponse {
    datetime: String,
}

fn fetch_time_from_url(url: &str) -> Option<(u8, u8, u16, u8, u8)> {
    let response = reqwest::blocking::get(url).ok()?;
    let json: WorldTimeApiResponse = response.json().ok()?;

    let datetime = json.datetime; // ISO 8601 format
    let parsed = chrono::DateTime::parse_from_rfc3339(&datetime).ok()?;
    let utc = parsed.with_timezone(&Utc);

    Some((
        utc.day() as u8,
        utc.month() as u8,
        utc.year() as u16,
        utc.hour() as u8,
        utc.minute() as u8,
    ))
}

// -- Deviation Checking Utilities --

fn check_sequential_low_deviation(a: u8, b: u8, c: u8) -> bool {
    let mut numbers = vec![a, b, c];
    numbers.sort();
    (numbers[1] - numbers[0]) <= 10 && (numbers[2] - numbers[1]) <= 10
}

fn check_pair_deviation_and_average(a: u8, b: u8, c: u8) -> Option<u8> {
    if (a as i16 - b as i16).abs() <= 10 {
        Some(((a as u16 + b as u16) / 2) as u8)
    } else if (a as i16 - c as i16).abs() <= 10 {
        Some(((a as u16 + c as u16) / 2) as u8)
    } else if (b as i16 - c as i16).abs() <= 10 {
        Some(((b as u16 + c as u16) / 2) as u8)
    } else {
        None
    }
}

// -- Main get_date_time() Function --

fn get_date_time(fallback_clock: &ClockHandle) -> (u8, u8, u16, u8, u8) {
    // Wait until fallback is ready
    while !fallback_clock.is_ready() {
        thread::sleep(Duration::from_millis(10));
    }

    let time_a = fetch_time_from_url("https://worldtimeapi.org/api/timezone/Europe/London");
    let time_b = fetch_time_from_url("https://timeapi.io/api/Time/current/zone?timeZone=Europe/London");
    let time_c = fetch_time_from_url("http://worldclockapi.com/api/json/utc/now"); // Placeholder, might need other API

    if let (Some(a), Some(b), Some(c)) = (time_a, time_b, time_c) {
        if a == b && b == c {
            return a;
        }

        if a.0 == b.0 && b.0 == c.0 && // day
           a.1 == b.1 && b.1 == c.1 && // month
           a.2 == b.2 && b.2 == c.2 && // year
           a.3 == b.3 && b.3 == c.3     // hour
        {
            if check_sequential_low_deviation(a.4, b.4, c.4) {
                let avg_min = ((a.4 as u16 + b.4 as u16 + c.4 as u16) / 3) as u8;
                return (a.0, a.1, a.2, a.3, avg_min);
            }
        }

        if let Some(avg_minute) = check_pair_deviation_and_average(a.4, b.4, c.4) {
            return (a.0, a.1, a.2, a.3, avg_minute);
        }
    }

    let fallback = fallback_clock.get_time();
    println!("[Fallback] Using software clock");
    (fallback.day, fallback.month, fallback.year, fallback.hour, fallback.min)
}

fn main() {
    let clock_handle = ClockHandle::start();
    let datetime = get_date_time(&clock_handle);

    println!(
        "Final UTC Time: {:02}/{:02}/{:04} {:02}:{:02}Z",
        datetime.0, datetime.1, datetime.2, datetime.3, datetime.4
    );
}
