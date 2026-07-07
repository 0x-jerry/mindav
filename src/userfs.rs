use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
    OpenOptions, ReadDirMeta,
};

#[derive(Clone)]
pub struct UserScopedFs<F: DavFileSystem + Clone> {
    inner: F,
    user: String,
}

impl<F: DavFileSystem + Clone> UserScopedFs<F> {
    pub fn new(inner: F, user: String) -> Self {
        Self { inner, user }
    }

    fn scoped_path(&self, path: &DavPath) -> FsResult<DavPath> {
        let path_str = path.as_url_string();
        let scoped = format!("/{}{}", self.user, path_str);
        DavPath::new(&scoped).map_err(|_| FsError::GeneralFailure)
    }
}

impl<F: DavFileSystem + Clone + Send + Sync> DavFileSystem for UserScopedFs<F> {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        Box::pin(async move {
            let scoped = self.scoped_path(path)?;
            self.inner.open(&scoped, options).await
        })
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        Box::pin(async move {
            let scoped = self.scoped_path(path)?;
            self.inner.read_dir(&scoped, meta).await
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        Box::pin(async move {
            let scoped = self.scoped_path(path)?;
            self.inner.metadata(&scoped).await
        })
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let scoped = self.scoped_path(path)?;
            self.inner.create_dir(&scoped).await
        })
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let scoped = self.scoped_path(path)?;
            self.inner.remove_dir(&scoped).await
        })
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let scoped = self.scoped_path(path)?;
            self.inner.remove_file(&scoped).await
        })
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let scoped_from = self.scoped_path(from)?;
            let scoped_to = self.scoped_path(to)?;
            self.inner.rename(&scoped_from, &scoped_to).await
        })
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let scoped_from = self.scoped_path(from)?;
            let scoped_to = self.scoped_path(to)?;
            self.inner.copy(&scoped_from, &scoped_to).await
        })
    }
}
