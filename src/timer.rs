use std::ops::AddAssign;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// Unfortunately Instant::now() panics in the browser
pub struct Timer {
    #[cfg(not(target_arch = "wasm32"))]
    instant: Instant,
    #[cfg(target_arch = "wasm32")]
    value: f64,
}

impl Timer {
    pub fn now() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            instant: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            value: web_sys::window().unwrap().performance().unwrap().now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.instant.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let millis = Timer::now().value - self.value;
            Duration::from_secs_f64(millis / 1000.0)
        }
    }
}

impl AddAssign<Duration> for Timer {
    fn add_assign(&mut self, rhs: Duration) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.instant += rhs;
        }

        #[cfg(target_arch = "wasm32")]
        {
            self.value += rhs.as_secs_f64() * 1000.0;
        }
    }
}
