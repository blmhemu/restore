#![deny(warnings)]

mod config;

use crate::config::Config;
use warp::Filter;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let config = Config::new(std::env::args()).await;
    if config.is_none() {
        return;
    }
    let config = config.unwrap();

    let dl_dir_router =
        warp::path!("files" / "dl" / ..).and(warp::fs::dir(config.serve_path.clone()));

    let ls_dir_router =
        warp::path!("files" / "ls" / ..).and(warp_fs::fs::ls_dir(config.serve_path.clone()));

    let mk_dir_router =
        warp::path!("files" / "mkdir" / ..).and(warp_fs::fs::mk_dir(config.serve_path.clone()));

    let rm_dir_router =
        warp::path!("files" / "rmdir" / ..).and(warp_fs::fs::rm_dir(config.serve_path.clone()));

    let rm_file_router =
        warp::path!("files" / "rm" / ..).and(warp_fs::fs::rm_file(config.serve_path.clone()));

    let mv_path_router =
        warp::path!("files" / "mv" / ..).and(warp_fs::fs::mv_path(config.serve_path.clone()));

    // Limit is 50 GB.
    let up_file_router = warp::path!("files" / "up" / ..)
        .and(warp_fs::fs::up_file(config.serve_path.clone(), 53687091200));

    let routes = dl_dir_router
        .or(ls_dir_router)
        .or(mk_dir_router)
        .or(rm_dir_router)
        .or(rm_file_router)
        .or(mv_path_router)
        .or(up_file_router);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
