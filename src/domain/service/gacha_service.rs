use crate::domain::interface::{IGachaEventRepository, IUserRepository};
use crate::domain::model::{Authorization, GachaEvent, GachaEventId, GachaType};
use crate::error::ServiceError;
use crate::wrapper::rand_gen::RandomGen;
use crate::wrapper::unixtime::UnixTime;
use serde::*;
use std::sync::Arc;

// ガチャ
pub struct GachaService {
    gacha_repo: Arc<dyn IGachaEventRepository + Sync + Send>,
    user_repo: Arc<dyn IUserRepository + Sync + Send>,
}

#[derive(Serialize)]
pub struct DailyGachaRecord {
    latest: Option<GachaEvent>,
    is_available: bool,
    next_gacha_time: UnixTime,
}

impl GachaService {
    pub fn new(
        gacha_repo: Arc<dyn IGachaEventRepository + Sync + Send>,
        user_repo: Arc<dyn IUserRepository + Sync + Send>,
    ) -> GachaService {
        GachaService {
            gacha_repo,
            user_repo,
        }
    }

    pub async fn get_latest_daily_event(
        &self,
        auth: Authorization,
    ) -> Result<serde_json::Value, ServiceError> {
        let auth_user = auth.require_auth()?;
        let mut user = self.user_repo.find_by_subject(&auth_user.subject).await?;

        match self
            .gacha_repo
            .find_by_user_type(&user.id, &GachaType::Daily)
            .await
        {
            Ok(r) => serde_json::to_value(&r).map_err(|err| {
                ServiceError::internal_server_error(failure::Error::from_boxed_compat(Box::new(
                    err,
                )))
            }),
            Err(err) if err.status_code == http::StatusCode::NOT_FOUND => {
                Ok(serde_json::json!(null))
            }
            Err(err) => Err(err),
        }
    }

    pub async fn get_daily_gacha_record(
        &self,
        auth: Authorization,
    ) -> Result<DailyGachaRecord, ServiceError> {
        let auth_user = auth.require_auth()?;
        let mut user = self.user_repo.find_by_subject(&auth_user.subject).await?;

        let latest = match self
            .gacha_repo
            .find_by_user_type(&user.id, &GachaType::Daily)
            .await
        {
            Err(err) if err.status_code == http::StatusCode::NOT_FOUND => Ok(None),
            r => r.map(|e| Some(e)),
        }?;
        let is_available = latest
            .clone()
            .map(|r| r.is_available_at(UnixTime::now_jst()))
            // 最後のガチャ記録が存在しなければavailableとする
            .unwrap_or(true);

        Ok(DailyGachaRecord {
            latest,
            is_available,
            next_gacha_time: UnixTime::now_jst(),
        })
    }

    pub async fn try_daily(&self, auth: Authorization) -> Result<serde_json::Value, ServiceError> {
        let auth_user = auth.require_auth()?;
        let mut user = self.user_repo.find_by_subject(&auth_user.subject).await?;
        let user_cloned = user.clone();

        match self
            .gacha_repo
            .find_by_user_type(&user.id, &GachaType::Daily)
            .await
        {
            Ok(event) if !event.is_available_at(UnixTime::now_jst()) => Err(
                ServiceError::bad_request(failure::err_msg("Daily Gacha Rate Limit Exceeded")),
            ),
            Err(err) if err.status_code == http::StatusCode::NOT_FOUND => Ok(()),
            Ok(_) => Ok(()),
            Err(err) => Err(err),
        }?;

        // 終端の16は含まない
        let n = RandomGen::range(5, 16);
        user.add_point(n);
        self.user_repo.save(user.clone()).await?;

        let event = GachaEvent {
            id: GachaEventId::new(),
            user_id: user.id,
            gacha_type: GachaType::Daily,
            created_at: UnixTime::now_jst(),
        };

        if let Err(err) = self.gacha_repo.create(event.clone()).await {
            warn!("Failed to create a new gacha event: {:?} {:?}", event, err);
            error!("{:?}", err);

            // 失敗したときはロールバックを試みる
            if let Err(err) = self.user_repo.save(user_cloned.clone()).await {
                // ロールバックに失敗した場合は不整合が起こるのでログだけ吐いておく
                error!("Failed to save the original user data: {:?}", user_cloned);
                error!("{:?}", err);

                return Err(ServiceError::internal_server_error(failure::err_msg(
                    "operation failed",
                )));
            }

            warn!("Rollback completed");

            return Err(err);
        };

        Ok(serde_json::json!({ "obtained": n }))
    }
}
