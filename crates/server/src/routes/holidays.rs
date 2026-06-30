//! Holiday-calendar endpoints. Any authed user reads the calendar; HR maintains
//! it. Dates are `"YYYY-MM-DD"` path values; the list is scoped to a `?year=`.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing,
};
use serde::Deserialize;
use time::Month;

use domain::model::Holiday;
use shared::{
    dto::holiday::{HolidayDto, SetHolidayRequest},
    validation::holiday::validate_holiday,
};

use crate::{
    app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, routes::parse_date,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/holidays", routing::get(list))
        .route("/holidays/{date}", routing::put(set).delete(remove))
}

#[derive(Deserialize)]
struct YearQuery {
    year: i32,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<YearQuery>,
) -> Result<Json<Vec<HolidayDto>>, AppError> {
    let from = time::Date::from_calendar_date(q.year, Month::January, 1)
        .map_err(|_| AppError::Validation(format!("invalid year '{}'", q.year)))?;
    let to = time::Date::from_calendar_date(q.year, Month::December, 31)
        .map_err(|_| AppError::Validation(format!("invalid year '{}'", q.year)))?;
    let holidays = state.holiday.list(auth.user_id, from, to).await?;
    Ok(Json(holidays.iter().map(dto::holiday_dto).collect()))
}

async fn set(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(date): Path<String>,
    Json(body): Json<SetHolidayRequest>,
) -> Result<Json<HolidayDto>, AppError> {
    let date = parse_date(&date)?;
    validate_holiday(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .holiday
        .set(auth.user_id, date, body.name.clone())
        .await?;
    Ok(Json(dto::holiday_dto(&Holiday {
        date,
        name: body.name,
    })))
}

async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(date): Path<String>,
) -> Result<StatusCode, AppError> {
    let date = parse_date(&date)?;
    state.holiday.remove(auth.user_id, date).await?;
    Ok(StatusCode::NO_CONTENT)
}
