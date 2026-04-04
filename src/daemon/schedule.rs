use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Scheduler {
    jobs: Vec<ScheduledJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub name: String,
    pub schedule: Schedule,
    pub action: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Schedule {
    EveryMinutes(u32),
    EveryHours(u32),
    DailyAt { hour: u32, minute: u32 },
    AfterIdle(u32),
}

impl Scheduler {
    pub fn new() -> Self {
        Self { jobs: Vec::new() }
    }

    pub fn with_defaults(mut self) -> Self {
        self.add_job(ScheduledJob {
            id: "auto-test".to_string(),
            name: "Periodic test run".to_string(),
            schedule: Schedule::EveryMinutes(30),
            action: "cargo test --quiet".to_string(),
            enabled: false,
            last_run: None,
        });
        self.add_job(ScheduledJob {
            id: "auto-dream".to_string(),
            name: "Memory consolidation (autoDream)".to_string(),
            schedule: Schedule::DailyAt { hour: 3, minute: 0 },
            action: "dream".to_string(),
            enabled: true,
            last_run: None,
        });
        self
    }

    pub fn add_job(&mut self, job: ScheduledJob) {
        self.jobs.push(job);
    }

    pub fn check_due(&mut self, idle_minutes: u64) -> Vec<ScheduledJob> {
        let now = Utc::now();
        let mut due = Vec::new();
        for job in &mut self.jobs {
            if !job.enabled { continue; }
            let is_due = match &job.schedule {
                Schedule::EveryMinutes(mins) => {
                    job.last_run.map(|last| now.signed_duration_since(last).num_minutes() >= *mins as i64).unwrap_or(true)
                }
                Schedule::EveryHours(hours) => {
                    job.last_run.map(|last| now.signed_duration_since(last).num_hours() >= *hours as i64).unwrap_or(true)
                }
                Schedule::DailyAt { hour, minute } => {
                    now.hour() == *hour && now.minute() == *minute && job.last_run.map(|last| last.date_naive() != now.date_naive()).unwrap_or(true)
                }
                Schedule::AfterIdle(mins) => idle_minutes >= *mins as u64,
            };
            if is_due {
                job.last_run = Some(now);
                due.push(job.clone());
            }
        }
        due
    }
}
