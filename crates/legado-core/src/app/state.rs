//! Service container extracted from the upstream desktop API module.

use crate::app::config::AppConfig;
use crate::service::{
    book_group_service::BookGroupService, book_service::BookService,
    book_source_service::BookSourceService, json_document_service::JsonDocumentService,
    local_epub_book::LocalEpubBookService, local_pdf_book::LocalPdfBookService,
    local_txt_book::LocalTxtBookService, reading_stats_service::ReadingStatsService,
    update_service::UpdateService, user_service::UserService,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub book_service: Arc<BookService>,
    pub book_source_service: Arc<BookSourceService>,
    pub user_service: Arc<UserService>,
    pub book_group_service: Arc<BookGroupService>,
    pub local_txt_book_service: Arc<LocalTxtBookService>,
    pub local_epub_book_service: Arc<LocalEpubBookService>,
    pub local_pdf_book_service: Arc<LocalPdfBookService>,
    pub json_document_service: Arc<JsonDocumentService>,
    pub reading_stats_service: Arc<ReadingStatsService>,
    pub update_service: Arc<UpdateService>,
}
