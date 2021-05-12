// Collection of useful utils copied from warp lib.

use crate::fs::Entry::{Directory, File, Symlink, Unknown};
use bytes::Buf;
use futures::stream::Stream;
use futures::{future, TryStreamExt};
use mime::Mime;
use mpart_async::server::MultipartStream;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::iter::Iterator;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::{DirEntry, OpenOptions};
use tokio::io::AsyncWriteExt;
use warp::http::StatusCode;
use warp::reply::Json;
use warp::{reject, Filter, Rejection};

pub type One<T> = (T,);

#[inline]
pub(crate) fn one<T>(val: T) -> One<T> {
    (val,)
}

// Silly wrapper since Arc<PathBuf> doesn't implement AsRef<Path> ;_;
#[derive(Clone, Debug)]
struct ArcPath(Arc<PathBuf>);

impl AsRef<Path> for ArcPath {
    fn as_ref(&self) -> &Path {
        (*self.0).as_ref()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct DirEntries(Vec<Entry>);

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Entry {
    File {
        path: PathBuf,
        size: u64,
    },

    Directory {
        path: PathBuf,
    },

    Symlink {
        path: PathBuf,
        target: Option<PathBuf>,
    },

    Unknown {
        path: PathBuf,
    },
}

impl Entry {
    async fn new(base: impl AsRef<Path>, entry: DirEntry) -> Self {
        let abs_path = entry.path();
        let path = abs_path.strip_prefix(base.as_ref()).unwrap().to_path_buf();
        let metadata = entry.metadata().await.unwrap();
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            return Directory { path };
        } else if file_type.is_file() {
            return File {
                path,
                size: metadata.len(),
            };
        } else if file_type.is_symlink() {
            return Symlink {
                target: tokio::fs::read_link(abs_path).await.ok(),
                path,
            };
        }
        Unknown { path }
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct FsQuery {
    pub(crate) to: String,
}

fn sanitize_path(base: impl AsRef<Path>, tail: &str) -> Result<PathBuf, Rejection> {
    let mut buf = PathBuf::from(base.as_ref());
    let p = match percent_decode_str(tail).decode_utf8() {
        Ok(p) => p,
        Err(err) => {
            tracing::debug!("dir: failed to decode route={:?}: {:?}", tail, err);
            return Err(reject::not_found());
        }
    };
    tracing::trace!("dir? base={:?}, route={:?}", base.as_ref(), p);
    for seg in p.split('/') {
        if seg.starts_with("..") {
            tracing::warn!("dir: rejecting segment starting with '..'");
            return Err(reject::not_found());
        } else if seg.contains('\\') {
            tracing::warn!("dir: rejecting segment containing with backslash (\\)");
            return Err(reject::not_found());
        } else {
            buf.push(seg);
        }
    }
    Ok(buf)
}

fn sanitized_path_filter(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<PathBuf>, Error = Rejection> + Clone {
    warp::path::tail().and_then(move |tail: warp::path::Tail| {
        future::ready(sanitize_path(base.as_ref(), tail.as_str()))
    })
}

fn dir_filter(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<ArcPath>, Error = Rejection> + Clone {
    sanitized_path_filter(base).and_then(|buf: PathBuf| async {
        let is_dir = tokio::fs::metadata(buf.clone())
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false);

        if !is_dir {
            tracing::trace!("dir: no such dir {:?}", buf);
            return Err(reject::not_found());
        }
        tracing::trace!("dir: {:?}", buf);
        Ok(ArcPath(Arc::new(buf)))
    })
}

async fn ls_dir_reply(base: ArcPath, dir_path: ArcPath) -> Result<Json, Infallible> {
    let mut dir_entries = vec![];

    let mut dir_contents = tokio::fs::read_dir(dir_path.0.as_ref()).await.unwrap();
    while let Some(entry) = dir_contents.next_entry().await.unwrap() {
        dir_entries.push(Entry::new(base.as_ref(), entry).await);
    }
    Ok(warp::reply::json(&DirEntries(dir_entries)))
}

fn with_base(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<ArcPath>, Error = Infallible> + Clone {
    warp::any().map(move || ArcPath(base.clone()))
}

pub fn ls_dir(
    path: impl Into<PathBuf>,
) -> impl Filter<Extract = One<Json>, Error = Rejection> + Clone {
    let base = Arc::new(path.into());
    warp::get()
        .and(with_base(base.clone()))
        .and(dir_filter(base))
        .and_then(ls_dir_reply)
}

fn new_dir_filter(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<ArcPath>, Error = Rejection> + Clone {
    sanitized_path_filter(base).and_then(|buf: PathBuf| async {
        let exists = tokio::fs::metadata(buf.clone()).await.is_ok();

        if exists {
            tracing::trace!("dir: already exists {:?}", buf);
            return Err(reject::not_found());
        }
        tracing::trace!("dir: new dir path {:?}", buf);
        Ok(ArcPath(Arc::new(buf)))
    })
}

/// Creates a single directory. Rejects if it is not a single dir.
async fn mk_dir_reply(new_dir_path: ArcPath) -> Result<StatusCode, Rejection> {
    match tokio::fs::create_dir(new_dir_path).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(reject::not_found()),
    }
}

pub fn mk_dir(
    path: impl Into<PathBuf>,
) -> impl Filter<Extract = One<StatusCode>, Error = Rejection> + Clone {
    let base = Arc::new(path.into());
    warp::put().and(new_dir_filter(base)).and_then(mk_dir_reply)
}

async fn rm_dir_reply(dir: ArcPath) -> Result<StatusCode, Rejection> {
    match tokio::fs::remove_dir_all(dir).await {
        Ok(_) => Ok(StatusCode::OK),
        Err(_) => Err(reject::not_found()),
    }
}

pub fn rm_dir(
    path: impl Into<PathBuf>,
) -> impl Filter<Extract = One<StatusCode>, Error = Rejection> + Clone {
    let base = Arc::new(path.into());
    warp::delete().and(dir_filter(base)).and_then(rm_dir_reply)
}

fn file_filter(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<ArcPath>, Error = Rejection> + Clone {
    sanitized_path_filter(base).and_then(|buf: PathBuf| async {
        tokio::fs::metadata(buf.clone())
            .await
            .map(|m| m.is_file())
            .map_or(Err(reject::not_found()), |_| Ok(ArcPath(Arc::new(buf))))
    })
}

fn valid_path_filter(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<ArcPath>, Error = Rejection> + Clone {
    sanitized_path_filter(base).and_then(|buf: PathBuf| async {
        tokio::fs::metadata(buf.clone())
            .await
            .map_or(Err(reject::not_found()), |_| Ok(ArcPath(Arc::new(buf))))
    })
}

async fn rm_file_reply(dir: ArcPath) -> Result<StatusCode, Rejection> {
    tokio::fs::remove_file(dir)
        .await
        .map_or(Err(reject::not_found()), |_| Ok(StatusCode::OK))
}

pub fn rm_file(
    path: impl Into<PathBuf>,
) -> impl Filter<Extract = One<StatusCode>, Error = Rejection> + Clone {
    let base = Arc::new(path.into());
    warp::delete()
        .and(file_filter(base))
        .and_then(rm_file_reply)
}

// Probably can do better by doing an ls ?
async fn get_available_name(base: &Path, name: &str) -> String {
    let file_stem = Path::new(name).file_stem().unwrap().to_str().unwrap();
    let file_ext = Path::new(name).extension().map(|r| r.to_str().unwrap());
    let mut counter: u32 = 0;
    let mut new_name = String::from(name);
    while tokio::fs::metadata(base.join(&new_name)).await.is_ok() {
        counter = counter + 1;
        new_name = format!("{}_{}", file_stem, counter);
        if let Some(ext) = file_ext {
            new_name = format!("{}.{}", new_name, ext);
        }
    }
    return new_name;
}

async fn up_file_reply(
    dir: ArcPath,
    mime: Mime,
    body: impl Stream<Item = Result<impl Buf, warp::Error>> + Unpin,
) -> Result<StatusCode, warp::Rejection> {
    let boundary = mime.get_param("boundary").map(|v| v.to_string()).unwrap();

    let mut stream = MultipartStream::new(
        boundary,
        body.map_ok(|mut buf| buf.copy_to_bytes(buf.remaining())),
    );

    while let Ok(Some(mut field)) = stream.try_next().await {
        if field.filename().is_err() {
            tracing::trace!("dir: no filename");
            return Err(reject::not_found());
        }
        let new_name = get_available_name(dir.as_ref(), field.filename().unwrap()).await;
        tracing::trace!("dir: upload name {:?}", field.name().unwrap());
        tracing::trace!("dir: upload available filename {:?}", new_name);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.as_ref().join(new_name))
            .await
            .unwrap();

        while let Ok(Some(bytes)) = field.try_next().await {
            file.write_all(&bytes).await.unwrap();
        }
        file.flush().await.unwrap();
    }

    Ok(StatusCode::OK)
}

pub fn up_file(
    path: impl Into<PathBuf>,
    byte_limit: u64,
) -> impl Filter<Extract = One<StatusCode>, Error = Rejection> + Clone {
    let base = Arc::new(path.into());
    warp::post()
        .and(dir_filter(base))
        .and(warp::header::<Mime>("content-type"))
        .and(warp::body::content_length_limit(byte_limit))
        .and(warp::body::stream())
        .and_then(up_file_reply)
}

fn query_dest_path_filter(
    base: Arc<PathBuf>,
) -> impl Filter<Extract = One<PathBuf>, Error = Rejection> + Clone {
    warp::query::<FsQuery>()
        .map(move |query: FsQuery| sanitize_path(base.as_ref(), &query.to).unwrap())
        .and_then(move |to_path| async {
            tokio::fs::metadata(&to_path)
                .await
                .map_or(Ok(to_path), |_| Err(reject::not_found()))
        })
}

async fn mv_path_reply(dir: ArcPath, target: PathBuf) -> Result<StatusCode, warp::Rejection> {
    tracing::trace!("dir: rename {:?} to {:?}", dir.as_ref(), target);
    tokio::fs::rename(dir.as_ref(), target)
        .await
        .map_or(Err(reject::not_found()), |_| Ok(StatusCode::OK))
}

pub fn mv_path(
    path: impl Into<PathBuf>,
) -> impl Filter<Extract = One<StatusCode>, Error = Rejection> + Clone {
    let base = Arc::new(path.into());
    let base_clone = base.clone();
    warp::post()
        .and(valid_path_filter(base.clone()))
        .and(query_dest_path_filter(base_clone))
        .and_then(mv_path_reply)
}
