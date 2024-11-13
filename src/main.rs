use anyhow::Error;
use ffmpeg_rs_raw::ffmpeg_sys_the_third::av_version_info;
use ffmpeg_rs_raw::rstr;
use log::{info, warn};

use crate::egress::Egress;

mod egress;
mod mixer;
mod speaker;

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    unsafe {
        info!("FFMPEG version={}", rstr!(av_version_info()));
    }

    let mut rooms = Vec::new();
    for r in vec!["c9987fa0-c21f-4fba-b2b9-332694120498".to_string()] {
        rooms.push(tokio::spawn(async move {
            let egress = Egress::new(r);
            if let Err(e) = egress.run().await {
                warn!("Error running egress {}", e);
            }
        }));
    }

    for j in rooms {
        let _ = j.await;
    }
    Ok(())
}
