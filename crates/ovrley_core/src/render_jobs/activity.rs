//! Keeps macOS from applying App Nap while a render operation is running.
//! The worker holds the activity through preparation, encoding and cleanup.
//! UserInitiated also prevents idle system sleep; display sleep stays allowed.

pub(super) use platform::RenderActivity;

#[cfg(target_os = "macos")]
mod platform {
    use objc2::rc::Retained;
    use objc2::runtime::{NSObjectProtocol, ProtocolObject};
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

    #[must_use = "hold the activity until rendering and cleanup finish"]
    pub struct RenderActivity(Retained<ProtocolObject<dyn NSObjectProtocol>>);

    impl RenderActivity {
        pub fn begin(reason: &str) -> Self {
            let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
                NSActivityOptions::UserInitiated,
                &NSString::from_str(reason),
            );
            log::info!("Render activity started: {reason}");
            Self(token)
        }
    }

    impl Drop for RenderActivity {
        fn drop(&mut self) {
            // SAFETY: this token came from the process singleton's beginActivity
            // and is ended exactly once, before its retained reference is released.
            unsafe { NSProcessInfo::processInfo().endActivity(&self.0) };
            log::info!("Render activity ended");
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    /// App Nap is specific to macOS; other platforms need no activity assertion.
    #[must_use = "hold the activity until rendering and cleanup finish"]
    pub struct RenderActivity;

    impl RenderActivity {
        pub fn begin(_reason: &str) -> Self {
            Self
        }
    }
}
