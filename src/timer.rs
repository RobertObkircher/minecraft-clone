use std::ops::AddAssign;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

/// Unfortunately Instant::now() panics in the browser
pub struct Timer {
    #[cfg(not(target_arch = "wasm32"))]
    instant: Instant,
    #[cfg(target_arch = "wasm32")]
    value: f64,
}

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
extern "C" {
    type Performance;
    #[wasm_bindgen (method , structural , js_class = "Performance" , js_name = now)]
    fn now(this: &Performance) -> f64;

    #[wasm_bindgen(js_name = performance)]
    static PERFORMANCE: Performance;
}

impl Timer {
    pub fn now() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            instant: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            value: PERFORMANCE.now(),
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
