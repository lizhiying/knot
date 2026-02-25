//! 内存峰值监控
//!
//! 通过 OS 级 API 获取进程 RSS（Resident Set Size），
//! 用于记录每页处理前后的内存变化和峰值。
//!
//! - macOS: `mach_task_basic_info`
//! - Linux: `/proc/self/status`
//! - 其他平台: 返回 0（不支持但不报错）

/// 获取当前进程的 RSS（字节）
pub fn current_rss_bytes() -> usize {
    #[cfg(target_os = "macos")]
    {
        macos_rss()
    }
    #[cfg(target_os = "linux")]
    {
        linux_rss()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

/// macOS 实现：通过 mach API 获取 RSS
#[cfg(target_os = "macos")]
fn macos_rss() -> usize {
    use std::mem;

    // mach_task_basic_info 结构体
    #[repr(C)]
    struct TaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: [u32; 2],   // time_value_t
        system_time: [u32; 2], // time_value_t
        policy: i32,
        suspend_count: i32,
    }

    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(
            target_task: u32,
            flavor: u32,
            task_info_out: *mut TaskBasicInfo,
            task_info_out_cnt: *mut u32,
        ) -> i32;
    }

    const MACH_TASK_BASIC_INFO: u32 = 20;

    unsafe {
        let mut info: TaskBasicInfo = mem::zeroed();
        let mut count = (mem::size_of::<TaskBasicInfo>() / mem::size_of::<u32>()) as u32;

        let result = task_info(
            mach_task_self(),
            MACH_TASK_BASIC_INFO,
            &mut info as *mut _,
            &mut count,
        );

        if result == 0 {
            info.resident_size as usize
        } else {
            0
        }
    }
}

/// Linux 实现：通过 /proc/self/status 获取 VmRSS
#[cfg(target_os = "linux")]
fn linux_rss() -> usize {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                // 格式: "VmRSS:     12345 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<usize>() {
                        return kb * 1024; // 转换为字节
                    }
                }
            }
        }
    }
    0
}

/// 内存快照：记录某个时间点的内存状态
#[derive(Debug, Clone, Copy)]
pub struct MemorySnapshot {
    /// RSS（字节）
    pub rss_bytes: usize,
}

impl MemorySnapshot {
    /// 采集当前内存快照
    pub fn now() -> Self {
        Self {
            rss_bytes: current_rss_bytes(),
        }
    }

    /// RSS 转换为 MB
    pub fn rss_mb(&self) -> f64 {
        self.rss_bytes as f64 / (1024.0 * 1024.0)
    }
}

/// 页面内存统计
#[derive(Debug, Clone, Copy)]
pub struct PageMemoryStats {
    /// 页面处理前的 RSS（字节）
    pub before_rss: usize,
    /// 页面处理后的 RSS（字节）
    pub after_rss: usize,
    /// 页面处理期间的 RSS 增量（正数=增长，负数=回落）
    pub delta_bytes: i64,
}

impl PageMemoryStats {
    /// 从两个快照计算
    pub fn from_snapshots(before: &MemorySnapshot, after: &MemorySnapshot) -> Self {
        Self {
            before_rss: before.rss_bytes,
            after_rss: after.rss_bytes,
            delta_bytes: after.rss_bytes as i64 - before.rss_bytes as i64,
        }
    }

    /// 增量转换为 MB
    pub fn delta_mb(&self) -> f64 {
        self.delta_bytes as f64 / (1024.0 * 1024.0)
    }
}
