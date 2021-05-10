// #![deny(warnings)]

mod fs;

use warp::Filter;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let dl_dir_router =
        warp::path!("files" / "dl" / ..).and(warp::fs::dir("/Users/hbollamreddi/sfolder"));

    let ls_dir_router =
        warp::path!("files" / "ls" / ..).and(crate::fs::ls_dir("/Users/hbollamreddi/sfolder"));

    let mk_dir_router =
        warp::path!("files" / "mkdir" / ..).and(crate::fs::mk_dir("/Users/hbollamreddi/sfolder"));

    let rm_dir_router =
        warp::path!("files" / "rmdir" / ..).and(crate::fs::rm_dir("/Users/hbollamreddi/sfolder"));

    let rm_file_router =
        warp::path!("files" / "rm" / ..).and(crate::fs::rm_file("/Users/hbollamreddi/sfolder"));

    let mv_path_router =
        warp::path!("files" / "mv" / ..).and(crate::fs::mv_path("/Users/hbollamreddi/sfolder"));

    // Limit is 50 GB.
    let up_file_router = warp::path!("files" / "up" / ..).and(crate::fs::up_file(
        "/Users/hbollamreddi/sfolder",
        53687091200,
    ));

    let routes = dl_dir_router
        .or(ls_dir_router)
        .or(mk_dir_router)
        .or(rm_dir_router)
        .or(rm_file_router)
        .or(mv_path_router)
        .or(up_file_router);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
