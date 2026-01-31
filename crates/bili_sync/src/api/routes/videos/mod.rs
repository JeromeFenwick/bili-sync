use std::collections::HashSet;

use anyhow::{Context, Result};
use axum::extract::{Extension, Path, Query};
use axum::routing::{get, post};
use axum::{Json, Router};
use bili_sync_entity::*;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, Order, PaginatorTrait,
    QueryFilter, QueryOrder, TransactionTrait, TryIntoModel,
};

use std::path::PathBuf;
use std::sync::Arc;

use crate::adapter::{VideoSource, VideoSourceEnum};
use crate::api::error::InnerApiError;
use crate::api::helper::{update_page_download_status, update_video_download_status};
use crate::api::request::{
    ResetFilteredVideoStatusRequest, ResetVideoStatusRequest, RetryPageTaskRequest, RetryVideoTaskRequest,
    SortOrder, UpdateFilteredVideoStatusRequest, UpdateVideoStatusRequest, VideoSortBy, VideosRequest,
};
use crate::api::response::{
    ClearAndResetVideoStatusResponse, PageInfo, ResetFilteredVideosResponse, ResetVideoResponse, SimplePageInfo,
    SimpleVideoInfo, UpdateFilteredVideoStatusResponse, UpdateVideoStatusResponse, VideoInfo, VideoResponse,
    VideosResponse,
};
use crate::api::wrapper::{ApiError, ApiResponse, ValidatedJson};
use crate::bilibili::{BiliClient, PageInfo as BiliPageInfo};
use crate::config::{PathSafeTemplate, TEMPLATE, VersionedConfig};
use crate::downloader::Downloader;
use crate::utils::download_context::DownloadContext;
use crate::utils::format_arg::{page_format_args, video_format_args};
use crate::error::ExecutionStatus;
use crate::utils::status::{PageStatus, VideoStatus};
use tracing;
use crate::workflow::{
    dispatch_download_page, fetch_page_danmaku, fetch_page_poster, fetch_page_subtitle, fetch_page_video,
    fetch_upper_face, fetch_video_poster, generate_page_nfo, generate_upper_nfo, generate_video_nfo,
};

pub(super) fn router() -> Router {
    Router::new()
        .route("/videos", get(get_videos))
        .route("/videos/{id}", get(get_video))
        .route(
            "/videos/{id}/clear-and-reset-status",
            post(clear_and_reset_video_status),
        )
        .route("/videos/{id}/reset-status", post(reset_video_status))
        .route("/videos/{id}/update-status", post(update_video_status))
        .route("/videos/{id}/retry-task", post(retry_video_task))
        .route("/pages/{id}/retry-task", post(retry_page_task))
        .route("/videos/reset-status", post(reset_filtered_video_status))
        .route("/videos/update-status", post(update_filtered_video_status))
}

/// 列出视频的基本信息，支持根据视频来源筛选、名称查找和分页
pub async fn get_videos(
    Extension(db): Extension<DatabaseConnection>,
    Query(params): Query<VideosRequest>,
) -> Result<ApiResponse<VideosResponse>, ApiError> {
    let mut query = video::Entity::find();
    for (field, column) in [
        (params.collection, video::Column::CollectionId),
        (params.favorite, video::Column::FavoriteId),
        (params.submission, video::Column::SubmissionId),
        (params.watch_later, video::Column::WatchLaterId),
    ] {
        if let Some(id) = field {
            query = query.filter(column.eq(id));
        }
    }
    if let Some(query_word) = params.query {
        query = query.filter(
            video::Column::Name
                .contains(&query_word)
                .or(video::Column::Bvid.contains(query_word)),
        );
    }
    if let Some(status_filter) = params.status_filter {
        query = query.filter(status_filter.to_video_query());
    }
    let total_count = query.clone().count(&db).await?;
    let (page, page_size) = if let (Some(page), Some(page_size)) = (params.page, params.page_size) {
        (page, page_size)
    } else {
        (0, 10)
    };

    // 排序逻辑：
    // - 如果显式指定 sort_by / sort_order，则按指定排序；
    // - 否则：
    //   - 如果存在来源筛选（收藏夹 / 合集 / 投稿 / 稍后再看），默认按订阅时间倒序；
    //   - 否则默认按下载时间倒序。
    let has_source_filter = params.collection.is_some()
        || params.favorite.is_some()
        || params.submission.is_some()
        || params.watch_later.is_some();

    let sort_by = params
        .sort_by
        .unwrap_or(if has_source_filter { VideoSortBy::SubscribeTime } else { VideoSortBy::DownloadTime });
    let sort_order = params.sort_order.unwrap_or(SortOrder::Desc);

    let order_column = match sort_by {
        VideoSortBy::PublishTime => video::Column::Pubtime,
        VideoSortBy::SubscribeTime => video::Column::Favtime,
        VideoSortBy::DownloadTime => video::Column::CreatedAt,
    };

    query = query.order_by(
        order_column,
        match sort_order {
            SortOrder::Asc => Order::Asc,
            SortOrder::Desc => Order::Desc,
        },
    );

    Ok(ApiResponse::ok(VideosResponse {
        videos: query.into_partial_model::<VideoInfo>().paginate(&db, page_size).fetch_page(page).await?,
        total_count,
    }))
}

pub async fn get_video(
    Path(id): Path<i32>,
    Extension(db): Extension<DatabaseConnection>,
) -> Result<ApiResponse<VideoResponse>, ApiError> {
    let (video_info, pages_info) = tokio::try_join!(
        video::Entity::find_by_id(id).into_partial_model::<VideoInfo>().one(&db),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(id))
            .order_by_asc(page::Column::Cid)
            .into_partial_model::<PageInfo>()
            .all(&db)
    )?;
    let Some(video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };
    Ok(ApiResponse::ok(VideoResponse {
        video: video_info,
        pages: pages_info,
    }))
}

pub async fn reset_video_status(
    Path(id): Path<i32>,
    Extension(db): Extension<DatabaseConnection>,
    Json(request): Json<ResetVideoStatusRequest>,
) -> Result<ApiResponse<ResetVideoResponse>, ApiError> {
    let (video_info, pages_info) = tokio::try_join!(
        video::Entity::find_by_id(id).into_partial_model::<VideoInfo>().one(&db),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(id))
            .order_by_asc(page::Column::Cid)
            .into_partial_model::<PageInfo>()
            .all(&db)
    )?;
    let Some(mut video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };
    let resetted_pages_info = pages_info
        .into_iter()
        .filter_map(|mut page_info| {
            let mut page_status = PageStatus::from(page_info.download_status);
            if (request.force && page_status.force_reset_failed()) || page_status.reset_failed() {
                page_info.download_status = page_status.into();
                Some(page_info)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut video_status = VideoStatus::from(video_info.download_status);
    let mut video_resetted = (request.force && video_status.force_reset_failed()) || video_status.reset_failed();
    if !resetted_pages_info.is_empty() {
        video_status.set(4, 0); //  将“分页下载”重置为 0
        video_resetted = true;
    }
    let resetted_videos_info = if video_resetted {
        video_info.download_status = video_status.into();
        vec![&video_info]
    } else {
        vec![]
    };
    let resetted = !resetted_videos_info.is_empty() || !resetted_pages_info.is_empty();
    if resetted {
        let txn = db.begin().await?;
        if !resetted_videos_info.is_empty() {
            // 只可能有 1 个元素，所以不用 batch
            update_video_download_status::<VideoInfo>(&txn, &resetted_videos_info, None).await?;
        }
        if !resetted_pages_info.is_empty() {
            update_page_download_status(&txn, &resetted_pages_info, Some(500)).await?;
        }
        txn.commit().await?;
    }
    Ok(ApiResponse::ok(ResetVideoResponse {
        resetted,
        video: video_info,
        pages: resetted_pages_info,
    }))
}

pub async fn clear_and_reset_video_status(
    Path(id): Path<i32>,
    Extension(db): Extension<DatabaseConnection>,
) -> Result<ApiResponse<ClearAndResetVideoStatusResponse>, ApiError> {
    let video_info = video::Entity::find_by_id(id).one(&db).await?;
    let Some(video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };
    let txn = db.begin().await?;
    let mut video_info = video_info.into_active_model();
    video_info.single_page = Set(None);
    video_info.download_status = Set(0);
    let video_info = video_info.update(&txn).await?;
    page::Entity::delete_many()
        .filter(page::Column::VideoId.eq(id))
        .exec(&txn)
        .await?;
    txn.commit().await?;
    let video_info = video_info.try_into_model()?;
    let warning = tokio::fs::remove_dir_all(&video_info.path)
        .await
        .context(format!("删除本地路径「{}」失败", video_info.path))
        .err()
        .map(|e| format!("{:#}", e));
    Ok(ApiResponse::ok(ClearAndResetVideoStatusResponse {
        warning,
        video: VideoInfo {
            id: video_info.id,
            bvid: video_info.bvid,
            name: video_info.name,
            upper_name: video_info.upper_name,
            should_download: video_info.should_download,
            is_paid_video: video_info.is_paid_video,
            download_status: video_info.download_status,
        },
    }))
}

pub async fn reset_filtered_video_status(
    Extension(db): Extension<DatabaseConnection>,
    Json(request): Json<ResetFilteredVideoStatusRequest>,
) -> Result<ApiResponse<ResetFilteredVideosResponse>, ApiError> {
    let mut query = video::Entity::find();
    for (field, column) in [
        (request.collection, video::Column::CollectionId),
        (request.favorite, video::Column::FavoriteId),
        (request.submission, video::Column::SubmissionId),
        (request.watch_later, video::Column::WatchLaterId),
    ] {
        if let Some(id) = field {
            query = query.filter(column.eq(id));
        }
    }
    if let Some(query_word) = request.query {
        query = query.filter(
            video::Column::Name
                .contains(&query_word)
                .or(video::Column::Bvid.contains(query_word)),
        );
    }
    if let Some(status_filter) = request.status_filter {
        query = query.filter(status_filter.to_video_query());
    }
    let all_videos = query.into_partial_model::<SimpleVideoInfo>().all(&db).await?;
    let all_pages = page::Entity::find()
        .filter(page::Column::VideoId.is_in(all_videos.iter().map(|v| v.id)))
        .into_partial_model::<SimplePageInfo>()
        .all(&db)
        .await?;
    let resetted_pages_info = all_pages
        .into_iter()
        .filter_map(|mut page_info| {
            let mut page_status = PageStatus::from(page_info.download_status);
            if (request.force && page_status.force_reset_failed()) || page_status.reset_failed() {
                page_info.download_status = page_status.into();
                Some(page_info)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let video_ids_with_resetted_pages: HashSet<i32> = resetted_pages_info.iter().map(|page| page.video_id).collect();
    let resetted_videos_info = all_videos
        .into_iter()
        .filter_map(|mut video_info| {
            let mut video_status = VideoStatus::from(video_info.download_status);
            let mut video_resetted =
                (request.force && video_status.force_reset_failed()) || video_status.reset_failed();
            if video_ids_with_resetted_pages.contains(&video_info.id) {
                video_status.set(4, 0); // 将"分页下载"重置为 0
                video_resetted = true;
            }
            if video_resetted {
                video_info.download_status = video_status.into();
                Some(video_info)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let has_video_updates = !resetted_videos_info.is_empty();
    let has_page_updates = !resetted_pages_info.is_empty();
    if has_video_updates || has_page_updates {
        let txn = db.begin().await?;
        if has_video_updates {
            update_video_download_status(&txn, &resetted_videos_info, Some(500)).await?;
        }
        if has_page_updates {
            update_page_download_status(&txn, &resetted_pages_info, Some(500)).await?;
        }
        txn.commit().await?;
    }
    Ok(ApiResponse::ok(ResetFilteredVideosResponse {
        resetted: has_video_updates || has_page_updates,
        resetted_videos_count: resetted_videos_info.len(),
        resetted_pages_count: resetted_pages_info.len(),
    }))
}

pub async fn update_video_status(
    Path(id): Path<i32>,
    Extension(db): Extension<DatabaseConnection>,
    ValidatedJson(request): ValidatedJson<UpdateVideoStatusRequest>,
) -> Result<ApiResponse<UpdateVideoStatusResponse>, ApiError> {
    let (video_info, mut pages_info) = tokio::try_join!(
        video::Entity::find_by_id(id).into_partial_model::<VideoInfo>().one(&db),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(id))
            .order_by_asc(page::Column::Cid)
            .into_partial_model::<PageInfo>()
            .all(&db)
    )?;
    let Some(mut video_info) = video_info else {
        return Err(InnerApiError::NotFound(id).into());
    };
    let mut video_status = VideoStatus::from(video_info.download_status);
    for update in &request.video_updates {
        video_status.set(update.status_index, update.status_value);
    }
    video_info.download_status = video_status.into();
    let mut updated_pages_info = Vec::new();
    let mut page_id_map = pages_info
        .iter_mut()
        .map(|page| (page.id, page))
        .collect::<std::collections::HashMap<_, _>>();
    for page_update in &request.page_updates {
        if let Some(page_info) = page_id_map.remove(&page_update.page_id) {
            let mut page_status = PageStatus::from(page_info.download_status);
            for update in &page_update.updates {
                page_status.set(update.status_index, update.status_value);
            }
            page_info.download_status = page_status.into();
            updated_pages_info.push(page_info);
        }
    }
    let has_video_updates = !request.video_updates.is_empty();
    let has_page_updates = !updated_pages_info.is_empty();
    let has_should_download_update = request.should_download.is_some();
    let has_is_paid_video_update = request.is_paid_video.is_some();
    if has_video_updates || has_page_updates || has_should_download_update || has_is_paid_video_update {
        let txn = db.begin().await?;
        if has_video_updates {
            update_video_download_status::<VideoInfo>(&txn, &[&video_info], None).await?;
        }
        if has_page_updates {
            update_page_download_status::<PageInfo>(&txn, &updated_pages_info, None).await?;
        }
        if has_should_download_update || has_is_paid_video_update {
            let video_model = video::Entity::find_by_id(video_info.id)
                .one(&txn)
                .await?
                .ok_or_else(|| InnerApiError::NotFound(video_info.id))?;
            let mut video_active_model: video::ActiveModel = video_model.into();
            if let Some(should_download) = request.should_download {
                video_active_model.should_download = Set(should_download);
            }
            if let Some(is_paid_video) = request.is_paid_video {
                video_active_model.is_paid_video = Set(is_paid_video);
                // 如果标记为收费视频，同时设置 should_download=false
                if is_paid_video {
                    video_active_model.should_download = Set(false);
                }
            }
            video_active_model.update(&txn).await?;
        }
        txn.commit().await?;
        // 重新查询以确保返回最新数据
        if has_video_updates || has_should_download_update || has_is_paid_video_update {
            if let Some(updated_video) = video::Entity::find_by_id(video_info.id)
                .into_partial_model::<VideoInfo>()
                .one(&db)
                .await?
            {
                video_info = updated_video;
            }
        }
        if has_page_updates {
            pages_info = page::Entity::find()
                .filter(page::Column::VideoId.eq(video_info.id))
                .order_by_asc(page::Column::Cid)
                .into_partial_model::<PageInfo>()
                .all(&db)
                .await?;
        }
    }
    Ok(ApiResponse::ok(UpdateVideoStatusResponse {
        success: has_video_updates || has_page_updates || has_should_download_update || has_is_paid_video_update,
        video: video_info,
        pages: pages_info,
    }))
}

pub async fn update_filtered_video_status(
    Extension(db): Extension<DatabaseConnection>,
    ValidatedJson(request): ValidatedJson<UpdateFilteredVideoStatusRequest>,
) -> Result<ApiResponse<UpdateFilteredVideoStatusResponse>, ApiError> {
    let mut query = video::Entity::find();
    
    // 如果提供了 video_ids，优先使用它来筛选（用于批量选择操作）
    if let Some(video_ids) = &request.video_ids {
        if !video_ids.is_empty() {
            query = query.filter(video::Column::Id.is_in(video_ids.clone()));
        } else {
            // 如果 video_ids 为空数组，直接返回空结果
            return Ok(ApiResponse::ok(UpdateFilteredVideoStatusResponse {
                success: false,
                updated_videos_count: 0,
                updated_pages_count: 0,
            }));
        }
    } else {
        // 否则使用原有的筛选逻辑
    for (field, column) in [
        (request.collection, video::Column::CollectionId),
        (request.favorite, video::Column::FavoriteId),
        (request.submission, video::Column::SubmissionId),
        (request.watch_later, video::Column::WatchLaterId),
    ] {
        if let Some(id) = field {
            query = query.filter(column.eq(id));
        }
    }
    if let Some(query_word) = request.query {
        query = query.filter(
            video::Column::Name
                .contains(&query_word)
                .or(video::Column::Bvid.contains(query_word)),
        );
    }
    if let Some(status_filter) = request.status_filter {
        query = query.filter(status_filter.to_video_query());
    }
    }
    
    let mut all_videos = query.into_partial_model::<SimpleVideoInfo>().all(&db).await?;
    let mut all_pages = page::Entity::find()
        .filter(page::Column::VideoId.is_in(all_videos.iter().map(|v| v.id)))
        .into_partial_model::<SimplePageInfo>()
        .all(&db)
        .await?;
    for video_info in all_videos.iter_mut() {
        let mut video_status = VideoStatus::from(video_info.download_status);
        for update in &request.video_updates {
            video_status.set(update.status_index, update.status_value);
        }
        video_info.download_status = video_status.into();
    }
    for page_info in all_pages.iter_mut() {
        let mut page_status = PageStatus::from(page_info.download_status);
        for update in &request.page_updates {
            page_status.set(update.status_index, update.status_value);
        }
        page_info.download_status = page_status.into();
    }
    let has_video_updates = !all_videos.is_empty();
    let has_page_updates = !all_pages.is_empty();
    let has_should_download_update = request.should_download.is_some();
    let has_is_paid_video_update = request.is_paid_video.is_some();
    if has_video_updates || has_page_updates || has_should_download_update || has_is_paid_video_update {
        let txn = db.begin().await?;
        if has_video_updates {
            update_video_download_status(&txn, &all_videos, Some(500)).await?;
        }
        if has_page_updates {
            update_page_download_status(&txn, &all_pages, Some(500)).await?;
        }
        if has_should_download_update || has_is_paid_video_update {
            let video_ids: Vec<i32> = all_videos.iter().map(|v| v.id).collect();
            for video_id in video_ids {
                let video_model = video::Entity::find_by_id(video_id)
                    .one(&txn)
                    .await?
                    .ok_or_else(|| InnerApiError::NotFound(video_id))?;
                let mut video_active_model: video::ActiveModel = video_model.into();
                if let Some(should_download) = request.should_download {
                    video_active_model.should_download = Set(should_download);
                }
                if let Some(is_paid_video) = request.is_paid_video {
                    video_active_model.is_paid_video = Set(is_paid_video);
                    // 如果标记为收费视频，同时设置 should_download=false
                    if is_paid_video {
                        video_active_model.should_download = Set(false);
                    }
                }
                video_active_model.update(&txn).await?;
            }
        }
        txn.commit().await?;
    }
    Ok(ApiResponse::ok(UpdateFilteredVideoStatusResponse {
        success: has_video_updates || has_page_updates || has_should_download_update || has_is_paid_video_update,
        updated_videos_count: all_videos.len(),
        updated_pages_count: all_pages.len(),
    }))
}

/// 从视频模型获取对应的 VideoSourceEnum
async fn get_video_source_from_model(
    video_model: &video::Model,
    db: &DatabaseConnection,
) -> Result<VideoSourceEnum, ApiError> {
    if let Some(collection_id) = video_model.collection_id {
        let collection = collection::Entity::find_by_id(collection_id)
            .one(db)
            .await?
            .ok_or_else(|| InnerApiError::NotFound(collection_id))?;
        return Ok(VideoSourceEnum::Collection(collection));
    }
    if let Some(favorite_id) = video_model.favorite_id {
        let favorite = favorite::Entity::find_by_id(favorite_id)
            .one(db)
            .await?
            .ok_or_else(|| InnerApiError::NotFound(favorite_id))?;
        return Ok(VideoSourceEnum::Favorite(favorite));
    }
    if let Some(watch_later_id) = video_model.watch_later_id {
        let watch_later = watch_later::Entity::find_by_id(watch_later_id)
            .one(db)
            .await?
            .ok_or_else(|| InnerApiError::NotFound(watch_later_id))?;
        return Ok(VideoSourceEnum::WatchLater(watch_later));
    }
    if let Some(submission_id) = video_model.submission_id {
        let submission = submission::Entity::find_by_id(submission_id)
            .one(db)
            .await?
            .ok_or_else(|| InnerApiError::NotFound(submission_id))?;
        return Ok(VideoSourceEnum::Submission(submission));
    }
    Err(InnerApiError::BadRequest("Video has no associated video source".to_string()).into())
}

/// 重试视频的单个任务
pub async fn retry_video_task(
    Path(id): Path<i32>,
    Extension(db): Extension<DatabaseConnection>,
    Extension(bili_client): Extension<Arc<BiliClient>>,
    ValidatedJson(request): ValidatedJson<RetryVideoTaskRequest>,
) -> Result<ApiResponse<UpdateVideoStatusResponse>, ApiError> {
    let video_model = video::Entity::find_by_id(id)
        .one(&db)
        .await?
        .ok_or_else(|| InnerApiError::NotFound(id))?;
    
    // 获取视频源
    let video_source = get_video_source_from_model(&video_model, &db).await?;
    
    // 获取配置和模板
    let config = VersionedConfig::get().read();
    let template = TEMPLATE.read();
    let downloader = Downloader::new(bili_client.client.clone());
    
    // 创建下载上下文
    let cx = DownloadContext::new(
        &bili_client,
        &video_source,
        &template,
        &db,
        &downloader,
        &config,
    );
    
    // 计算路径
    let base_path = if !video_model.path.is_empty() {
        PathBuf::from(&video_model.path)
    } else {
        video_source.path().join(
            template
                .path_safe_render("video", &video_format_args(&video_model, &config.time_format))
                .map_err(|e| InnerApiError::BadRequest(format!("Template render error: {}", e)))?,
        )
    };
    let upper_id = video_model.upper_id.to_string();
    let base_upper_path = config
        .upper_path
        .join(upper_id.chars().next().ok_or_else(|| InnerApiError::BadRequest("upper_id is empty".to_string()))?.to_string())
        .join(upper_id);
    let is_single_page = video_model.single_page.ok_or_else(|| InnerApiError::BadRequest("single_page is null".to_string()))?;
    
    // 确保视频源目录存在（与定时任务使用相同的规则）
    video_source.create_dir_all().await
        .map_err(|e| {
            tracing::error!("处理视频「{}」创建视频源目录失败: {}", &video_model.name, e);
            InnerApiError::BadRequest(format!("Failed to create video source directory: {}", e))
        })?;
    
    // 注意：不预先创建 base_path 和 base_upper_path，让下载函数自动创建（与定时任务保持一致）
    // downloader.fetch() 和 generate_nfo() 会自动创建所需的父目录
    
    // 根据 task_index 调用对应的函数
    let result = match request.task_index {
        0 => {
            // 下载视频封面
            let poster_path = base_path.join("poster.jpg");
            let fanart_path = base_path.join("fanart.jpg");
            fetch_video_poster(
                !is_single_page && !config.skip_option.no_poster,
                &video_model,
                poster_path.clone(),
                fanart_path.clone(),
                cx,
            )
            .await
        }
        1 => {
            // 生成视频信息的 nfo
            generate_video_nfo(
                !is_single_page && !config.skip_option.no_video_nfo,
                &video_model,
                base_path.join("tvshow.nfo"),
                cx,
            )
            .await
        }
        2 => {
            // 下载 UP 主头像
            let upper_face_path = base_upper_path.join("folder.jpg");
            fetch_upper_face(
                !config.skip_option.no_upper,
                &video_model,
                upper_face_path.clone(),
                cx,
            )
            .await
        }
        3 => {
            // 生成 UP 主信息的 nfo
            generate_upper_nfo(
                !config.skip_option.no_upper,
                &video_model,
                base_upper_path.join("person.nfo"),
                cx,
            )
            .await
        }
        4 => {
            // 分页下载任务需要特殊处理，这里触发分页下载
            // 获取所有分页
            let page_models = page::Entity::find()
                .filter(page::Column::VideoId.eq(id))
                .order_by_asc(page::Column::Cid)
                .all(&db)
                .await?;
            
            // 调用 dispatch_download_page 直接处理分页下载
            dispatch_download_page(true, &video_model, page_models, &base_path, cx).await
        }
        _ => return Err(InnerApiError::BadRequest(format!("Invalid task_index: {}", request.task_index)).into()),
    };
    
    // 更新状态（与定时任务使用相同的逻辑）
    let mut video_status = VideoStatus::from(video_model.download_status);
    let result_status = result?;
    
    // 记录日志（与定时任务使用相同的格式）
    let task_names = ["封面", "详情", "作者头像", "作者详情", "分页下载"];
    if let Some(task_name) = task_names.get(request.task_index) {
        match &result_status {
            ExecutionStatus::Skipped => {
                tracing::info!("处理视频「{}」{}已成功过，跳过", &video_model.name, task_name);
            }
            ExecutionStatus::Succeeded => {
                tracing::info!("处理视频「{}」{}成功", &video_model.name, task_name);
            }
            ExecutionStatus::Ignored(e) => {
                tracing::error!(
                    "处理视频「{}」{}出现常见错误，已忽略：{:#}",
                    &video_model.name, task_name, e
                );
            }
            ExecutionStatus::Failed(e) => {
                tracing::error!("处理视频「{}」{}失败：{:#}", &video_model.name, task_name, e);
            }
            ExecutionStatus::Fixed(_) => unreachable!(),
        }
    }
    
    // 创建一个只包含当前任务结果的数组，其他位置用当前状态填充
    let current_statuses: [u32; 5] = video_status.into();
    let mut all_results = [
        ExecutionStatus::Fixed(current_statuses[0]),
        ExecutionStatus::Fixed(current_statuses[1]),
        ExecutionStatus::Fixed(current_statuses[2]),
        ExecutionStatus::Fixed(current_statuses[3]),
        ExecutionStatus::Fixed(current_statuses[4]),
    ];
    all_results[request.task_index] = result_status;
    video_status.update_status(&all_results);
    
    // 在移动 video_model 之前保存路径信息
    let should_save_path = video_model.path.is_empty();
    let mut video_active_model: video::ActiveModel = video_model.into();
    video_active_model.download_status = Set(video_status.into());
    // 如果路径为空，保存计算出的路径（与定时任务一致）
    if should_save_path {
        video_active_model.path = Set(base_path.to_string_lossy().to_string());
    }
    video_active_model.save(&db).await?;
    
    // 重新查询更新后的数据
    let (video_info, pages_info) = tokio::try_join!(
        video::Entity::find_by_id(id).into_partial_model::<VideoInfo>().one(&db),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(id))
            .order_by_asc(page::Column::Cid)
            .into_partial_model::<PageInfo>()
            .all(&db)
    )?;
    
    Ok(ApiResponse::ok(UpdateVideoStatusResponse {
        success: true,
        video: video_info.ok_or_else(|| InnerApiError::NotFound(id))?,
        pages: pages_info,
    }))
}

/// 重试分页的单个任务
pub async fn retry_page_task(
    Path(id): Path<i32>,
    Extension(db): Extension<DatabaseConnection>,
    Extension(bili_client): Extension<Arc<BiliClient>>,
    ValidatedJson(request): ValidatedJson<RetryPageTaskRequest>,
) -> Result<ApiResponse<UpdateVideoStatusResponse>, ApiError> {
    let page_model = page::Entity::find_by_id(id)
        .one(&db)
        .await?
        .ok_or_else(|| InnerApiError::NotFound(id))?;
    
    let video_id = page_model.video_id;
    let video_model = video::Entity::find_by_id(video_id)
        .one(&db)
        .await?
        .ok_or_else(|| InnerApiError::NotFound(video_id))?;
    
    // 获取视频源
    let video_source = get_video_source_from_model(&video_model, &db).await?;
    
    // 获取配置和模板
    let config = VersionedConfig::get().read();
    let template = TEMPLATE.read();
    let downloader = Downloader::new(bili_client.client.clone());
    
    // 创建下载上下文
    let cx = DownloadContext::new(
        &bili_client,
        &video_source,
        &template,
        &db,
        &downloader,
        &config,
    );
    
    // 计算路径
    let is_single_page = video_model.single_page.ok_or_else(|| InnerApiError::BadRequest("single_page is null".to_string()))?;
    let (base_path, base_name): (PathBuf, String) = if let Some(old_video_path) = &page_model.path
        && !old_video_path.is_empty()
    {
        let old_video_path = std::path::Path::new(old_video_path);
        let old_video_filename = old_video_path
            .file_name()
            .ok_or_else(|| InnerApiError::BadRequest("invalid page path format".to_string()))?
            .to_string_lossy();
        if is_single_page {
            (
                old_video_path.parent().ok_or_else(|| InnerApiError::BadRequest("invalid page path format".to_string()))?.to_path_buf(),
                old_video_filename.trim_end_matches(".mp4").to_string(),
            )
        } else {
            (
                old_video_path
                    .parent()
                    .and_then(|p| p.parent())
                    .ok_or_else(|| InnerApiError::BadRequest("invalid page path format".to_string()))?
                    .to_path_buf(),
                old_video_filename
                    .rsplit_once(" - ")
                    .ok_or_else(|| InnerApiError::BadRequest("invalid page path format".to_string()))?
                    .0
                    .to_string(),
            )
        }
    } else {
        let video_base_path = if !video_model.path.is_empty() {
            PathBuf::from(&video_model.path)
        } else {
            video_source.path().join(
                template
                    .path_safe_render("video", &video_format_args(&video_model, &config.time_format))
                    .map_err(|e| InnerApiError::BadRequest(format!("Template render error: {}", e)))?,
            )
        };
        let page_name = template
            .path_safe_render("page", &page_format_args(&video_model, &page_model, &config.time_format))
            .map_err(|e| InnerApiError::BadRequest(format!("Template render error: {}", e)))?;
        (video_base_path, page_name)
    };
    
    // 确保视频源目录存在（与定时任务使用相同的规则）
    video_source.create_dir_all().await
        .map_err(|e| {
            tracing::error!("处理视频「{}」第 {} 页创建视频源目录失败: {}", &video_model.name, page_model.pid, e);
            InnerApiError::BadRequest(format!("Failed to create video source directory: {}", e))
        })?;
    
    // 注意：不预先创建 base_path 和 Season 1 目录，让下载函数自动创建（与定时任务保持一致）
    // downloader.fetch() 和 generate_nfo() 会自动创建所需的父目录
    
    let (poster_path, video_path, nfo_path, danmaku_path, fanart_path, subtitle_path): (PathBuf, PathBuf, PathBuf, PathBuf, Option<PathBuf>, PathBuf) = if is_single_page {
        (
            base_path.join(format!("{}-poster.jpg", &base_name)),
            base_path.join(format!("{}.mp4", &base_name)),
            base_path.join(format!("{}.nfo", &base_name)),
            base_path.join(format!("{}.zh-CN.default.ass", &base_name)),
            Some(base_path.join(format!("{}-fanart.jpg", &base_name))),
            base_path.join(format!("{}.srt", &base_name)),
        )
    } else {
        (
            base_path.join("Season 1").join(format!("{} - S01E{:0>2}-thumb.jpg", &base_name, page_model.pid)),
            base_path.join("Season 1").join(format!("{} - S01E{:0>2}.mp4", &base_name, page_model.pid)),
            base_path.join("Season 1").join(format!("{} - S01E{:0>2}.nfo", &base_name, page_model.pid)),
            base_path.join("Season 1").join(format!("{} - S01E{:0>2}.zh-CN.default.ass", &base_name, page_model.pid)),
            None,
            base_path.join("Season 1").join(format!("{} - S01E{:0>2}.srt", &base_name, page_model.pid)),
        )
    };
    
    let dimension = match (page_model.width, page_model.height) {
        (Some(width), Some(height)) => Some(crate::bilibili::Dimension {
            width,
            height,
            rotate: 0,
        }),
        _ => None,
    };
    let page_info = BiliPageInfo {
        cid: page_model.cid,
        duration: page_model.duration,
        dimension,
        ..Default::default()
    };
    
    // 根据 task_index 调用对应的函数
    let result = match request.task_index {
        0 => {
            // 下载分页封面
            fetch_page_poster(
                !config.skip_option.no_poster,
                &video_model,
                &page_model,
                poster_path.clone(),
                fanart_path.clone(),
                cx,
            )
            .await
        }
        1 => {
            // 下载分页视频
            fetch_page_video(
                true,
                &video_model,
                &page_info,
                &video_path,
                cx,
            )
            .await
        }
        2 => {
            // 生成分页视频信息的 nfo
            generate_page_nfo(
                !config.skip_option.no_video_nfo,
                &video_model,
                &page_model,
                nfo_path,
                cx,
            )
            .await
        }
        3 => {
            // 下载分页弹幕
            fetch_page_danmaku(
                !config.skip_option.no_danmaku,
                &video_model,
                &page_info,
                danmaku_path,
                cx,
            )
            .await
        }
        4 => {
            // 下载分页字幕
            fetch_page_subtitle(
                !config.skip_option.no_subtitle,
                &video_model,
                &page_info,
                &subtitle_path,
                cx,
            )
            .await
        }
        _ => return Err(InnerApiError::BadRequest(format!("Invalid task_index: {}", request.task_index)).into()),
    };
    
    // 更新状态（与定时任务使用相同的逻辑）
    let mut page_status = PageStatus::from(page_model.download_status);
    let result_status = result?;
    
    // 记录日志（与定时任务使用相同的格式）
    let task_names = ["封面", "视频", "详情", "弹幕", "字幕"];
    if let Some(task_name) = task_names.get(request.task_index) {
        match &result_status {
            ExecutionStatus::Skipped => {
                tracing::info!(
                    "处理视频「{}」第 {} 页{}已成功过，跳过",
                    &video_model.name, page_model.pid, task_name
                );
            }
            ExecutionStatus::Succeeded => {
                tracing::info!(
                    "处理视频「{}」第 {} 页{}成功",
                    &video_model.name, page_model.pid, task_name
                );
            }
            ExecutionStatus::Ignored(e) => {
                tracing::error!(
                    "处理视频「{}」第 {} 页{}出现常见错误，已忽略：{:#}",
                    &video_model.name, page_model.pid, task_name, e
                );
            }
            ExecutionStatus::Failed(e) => {
                tracing::error!(
                    "处理视频「{}」第 {} 页{}失败：{:#}",
                    &video_model.name, page_model.pid, task_name, e
                );
            }
            ExecutionStatus::Fixed(_) => unreachable!(),
        }
    }
    
    // 创建一个只包含当前任务结果的数组，其他位置用当前状态填充
    let current_statuses: [u32; 5] = page_status.into();
    let mut all_results = [
        ExecutionStatus::Fixed(current_statuses[0]),
        ExecutionStatus::Fixed(current_statuses[1]),
        ExecutionStatus::Fixed(current_statuses[2]),
        ExecutionStatus::Fixed(current_statuses[3]),
        ExecutionStatus::Fixed(current_statuses[4]),
    ];
    all_results[request.task_index] = result_status;
    page_status.update_status(&all_results);
    
    let mut page_active_model: page::ActiveModel = page_model.into();
    page_active_model.download_status = Set(page_status.into());
    // 保存路径（与定时任务一致）
    page_active_model.path = Set(Some(video_path.to_string_lossy().to_string()));
    page_active_model.save(&db).await?;
    
    // 如果重试的是分页下载任务（task_index=1），还需要更新视频的"分页下载"状态
    if request.task_index == 1 {
        let mut video_status = VideoStatus::from(video_model.download_status);
        // 检查所有分页的下载状态，取最小值
        let pages = page::Entity::find()
            .filter(page::Column::VideoId.eq(video_id))
            .all(&db)
            .await?;
        let mut min_status = 7u32; // STATUS_OK
        for page in pages {
            let page_status = PageStatus::from(page.download_status);
            let separate_status: [u32; 5] = page_status.into();
            min_status = min_status.min(separate_status[1]); // task_index 1 是视频下载
        }
        video_status.set(4, min_status); // 视频的 task_index 4 是分页下载
        let mut video_active_model: video::ActiveModel = video_model.into();
        video_active_model.download_status = Set(video_status.into());
        video_active_model.save(&db).await?;
    }
    
    // 重新查询更新后的数据
    let (video_info, pages_info) = tokio::try_join!(
        video::Entity::find_by_id(video_id).into_partial_model::<VideoInfo>().one(&db),
        page::Entity::find()
            .filter(page::Column::VideoId.eq(video_id))
            .order_by_asc(page::Column::Cid)
            .into_partial_model::<PageInfo>()
            .all(&db)
    )?;
    
    Ok(ApiResponse::ok(UpdateVideoStatusResponse {
        success: true,
        video: video_info.ok_or_else(|| InnerApiError::NotFound(video_id))?,
        pages: pages_info,
    }))
}
