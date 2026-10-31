//! Serveur MCP local : l'egui reste propriétaire de l'image et des mesures.
//! Les outils envoient des commandes bornées à la boucle UI, qui seule peut
//! figer une capture et modifier la session.

use std::{
    io::Cursor,
    sync::{
        mpsc::{Receiver, Sender},
        Arc, Mutex,
    },
};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use rmcp::schemars;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ErrorData,
};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::{geometry::{Measurement, Point}, storage::Calibration};
use std::path::PathBuf;

pub const MCP_ENDPOINT: &str = "http://127.0.0.1:8765/mcp";
fn parse_mcp_port(value: Option<&str>) -> Result<u16, String> {
    match value {
        None => Ok(8765),
        Some(value) => value.parse::<u16>().ok().filter(|port| *port > 0)
            .ok_or_else(|| "DPER_MEASURE_MCP_PORT doit être un port entier entre 1 et 65535.".into()),
    }
}

pub fn configured_endpoint() -> String {
    parse_mcp_port(std::env::var("DPER_MEASURE_MCP_PORT").ok().as_deref())
        .map(|port| format!("http://127.0.0.1:{port}/mcp"))
        .unwrap_or_else(|_| MCP_ENDPOINT.into())
}

#[derive(Clone, Debug)]
pub enum ServerStatus {
    Starting,
    Running,
    Stopped,
    Failed(String),
}

pub struct ServerHandle {
    stop: Option<oneshot::Sender<()>>,
    status: Arc<Mutex<ServerStatus>>,
}

impl ServerHandle {
    pub fn status(&self) -> ServerStatus {
        self.status.lock().map(|value| value.clone())
            .unwrap_or_else(|_| ServerStatus::Failed("État MCP indisponible".into()))
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

#[derive(Clone)]
pub struct Bridge {
    requests: Sender<UiRequest>,
    repaint: egui::Context,
}

pub type UiRequestReceiver = Receiver<UiRequest>;

pub enum InspectionRequest {
    Drawing(DrawingRequest),
    GetState,
    InstallPlan { plan: crate::dper_drawing::DrawingControlPlan, expected_revision: u64 },
    ProposeInterpretation(ProposeInterpretationArgs),
    Register(RegisterInspectionArgs),
    Inspect(InspectFeatureArgs),
}

#[derive(Clone, Debug)]
pub enum DrawingRequest {
    Source(GeometryPageArgs),
    InstallCatalog { catalog: crate::dper_pdf_geometry::DrawingGeometryCatalog, expected_revision: u64 },
    List(ListGeometryArgs),
    DefineView(DefineDrawingViewArgs),
    Candidates(BindingCandidatesArgs),
    Select(SelectBindingArgs),
    InspectBound(InspectBoundFeatureArgs),
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct GeometryPageArgs {
    /// One-based page of the active source PDF, from 1 to 20.
    pub page: u32,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ListGeometryArgs {
    pub page: u32,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
    /// Optional primitive kind: line, polyline, closed_polygon, or circle_candidate.
    pub kind: Option<String>,
    /// Optional exact primitive ID; identifiers are source-specific proposals.
    pub geometry_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct DefineDrawingViewArgs {
    pub expected_revision: u64,
    pub page: u32,
    pub label: String,
    pub feature_ids: Vec<String>,
    pub geometry_ids: Vec<String>,
    /// Replace this explicit view; omit to define a new scope.
    pub replace_view_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct BindingCandidatesArgs {
    pub feature_id: String,
    pub view_id: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SelectBindingArgs {
    pub expected_revision: u64,
    pub feature_id: String,
    pub view_id: String,
    pub candidate_id: String,
    /// Cite the actual drawing evidence for this choice; selecting never grants UI review.
    pub justification: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct InspectBoundFeatureArgs {
    #[serde(default)]
    pub source_measurement_ids: Vec<u64>,
    pub capture_id: String,
    pub expected_revision: u64,
    pub feature_id: String,
    pub group: String,
    pub options: Option<crate::dper_vision::VisionOptions>,
    /// False previews; true records observed pixels in a native measurement.
    #[serde(default)]
    pub commit: bool,
}

#[derive(Deserialize)]
struct DrawingGeometrySource {
    path: String,
    document_fingerprint: String,
    page: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalyzeDrawingArgs {
    pub path: String,
    pub expected_revision: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeInterpretationArgs {
    pub expected_revision: u64,
    pub feature_id: String,
    pub kind: crate::dper_drawing::DrawingDimensionKind,
    pub unit: Option<crate::dper_drawing::DrawingUnit>,
    /// Cite the drawing evidence supporting this proposal; it remains unconfirmed.
    pub source_note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RegisterInspectionArgs {
    pub capture_id: String,
    pub expected_revision: u64,
    /// Assisted landmarks in drawing coordinates and native image pixels.
    pub correspondences: Vec<crate::dper_registration::Correspondence>,
    pub maximum_error_px: f64,
    /// Optional explicit drawing view to scope registration for bound inspection.
    pub drawing_view_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InspectFeatureArgs {
    #[serde(default)]
    pub source_measurement_ids: Vec<u64>,
    pub capture_id: String,
    pub expected_revision: u64,
    pub feature_id: String,
    pub group: String,
    /// Search geometry in the registered drawing frame, never a measured value.
    pub geometry_in_drawing: crate::dper_vision::InspectionGeometry,
    pub options: Option<crate::dper_vision::VisionOptions>,
    /// False previews; true records observed contour points in the native UI.
    #[serde(default)]
    pub commit: bool,
}

pub enum UiRequest {
    Inspection {
        request: InspectionRequest,
        reply: oneshot::Sender<Result<serde_json::Value, String>>,
    },
    GetContext {
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    FreezeImage {
        reply: oneshot::Sender<Result<ImagePayload, String>>,
    },
    GetCrop {
        capture_id: String,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        reply: oneshot::Sender<Result<ImagePayload, String>>,
    },
    AddMeasurements {
        capture_id: String,
        expected_revision: Option<u64>,
        measurements: Vec<MeasurementInput>,
        reply: oneshot::Sender<Result<Vec<AddedMeasurement>, String>>,
    },
    UpdateMeasurements {
        capture_id: String,
        expected_revision: Option<u64>,
        updates: Vec<MeasurementUpdateInput>,
        reply: oneshot::Sender<Result<Vec<MeasurementView>, String>>,
    },
    DeleteMeasurements {
        capture_id: String,
        expected_revision: Option<u64>,
        ids: Vec<u64>,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    ClearMeasurements {
        capture_id: String,
        expected_revision: Option<u64>,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    MeasurementHistory {
        action: String,
        expected_revision: Option<u64>,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    CancelActiveAction {
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    SelectMeasurement {
        capture_id: String,
        id: Option<u64>,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    SetCalibration {
        capture_id: String,
        expected_revision: Option<u64>,
        reference_type: String,
        points: Vec<NativePoint>,
        known_mm: f64,
        expanded_uncertainty_mm: Option<f64>,
        reply: oneshot::Sender<Result<Calibration, String>>,
    },
    ListCameras {
        reply: oneshot::Sender<Result<Vec<CameraView>, String>>,
    },
    StartCamera {
        index: usize,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    StreamControl {
        action: String,
        expected_revision: Option<u64>,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    OpenImage {
        path: PathBuf,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    OpenProject {
        path: PathBuf,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    SaveProject {
        path: PathBuf,
        overwrite: bool,
        reply: oneshot::Sender<Result<String, String>>,
    },
    CapturePhoto {
        path: PathBuf,
        overwrite: bool,
        reply: oneshot::Sender<Result<String, String>>,
    },
    SetWorkspace {
        optical_setup: Option<String>,
        measurement_tool: Option<String>,
        show_grid: Option<bool>,
        show_center_guides: Option<bool>,
        zoom_percent: Option<f32>,
        reply: oneshot::Sender<Result<McpContext, String>>,
    },
    PrepareReport {
        folder: Option<PathBuf>,
        reply: oneshot::Sender<Result<ReportPreparation, String>>,
    },
    ExportReport {
        args: ExportReportArgs,
        reply: oneshot::Sender<Result<ReportExportResult, String>>,
    },
}

pub fn create_bridge(repaint: egui::Context) -> (Bridge, UiRequestReceiver) {
    let (requests, receiver) = std::sync::mpsc::channel();
    (Bridge { requests, repaint }, receiver)
}

impl Bridge {
    async fn inspection(&self, request: InspectionRequest) -> Result<serde_json::Value, String> {
        self.send(|reply| UiRequest::Inspection { request, reply }).await
    }
    async fn send<T: Send + 'static>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T, String>>) -> UiRequest,
    ) -> Result<T, String> {
        let (reply, response) = oneshot::channel();
        self.requests.send(build(reply))
            .map_err(|_| "L'application DPER_Measure n'est plus active.".to_string())?;
        self.repaint.request_repaint();
        response.await
            .map_err(|_| "La réponse de l'application a été interrompue.".to_string())?
    }

    async fn get_context(&self) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::GetContext { reply }).await
    }

    async fn freeze_image(&self) -> Result<ImagePayload, String> {
        self.send(|reply| UiRequest::FreezeImage { reply }).await
    }

    async fn get_crop(&self, args: CropImageArgs) -> Result<ImagePayload, String> {
        self.send(|reply| UiRequest::GetCrop {
            capture_id: args.capture_id,
            x: args.x,
            y: args.y,
            width: args.width,
            height: args.height,
            reply,
        }).await
    }

    async fn add_measurements(&self, args: AddMeasurementsArgs) -> Result<Vec<AddedMeasurement>, String> {
        self.send(|reply| UiRequest::AddMeasurements {
            capture_id: args.capture_id, expected_revision: args.expected_revision,
            measurements: args.measurements, reply,
        }).await
    }

    async fn update_measurements(&self, args: UpdateMeasurementsArgs) -> Result<Vec<MeasurementView>, String> {
        self.send(|reply| UiRequest::UpdateMeasurements {
            capture_id: args.capture_id, expected_revision: args.expected_revision,
            updates: args.updates, reply,
        }).await
    }

    async fn delete_measurements(&self, args: DeleteMeasurementsArgs) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::DeleteMeasurements {
            capture_id: args.capture_id, expected_revision: args.expected_revision, ids: args.ids, reply,
        }).await
    }

    async fn clear_measurements(&self, args: CaptureIdArgs) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::ClearMeasurements {
            capture_id: args.capture_id, expected_revision: args.expected_revision, reply,
        }).await
    }

    async fn measurement_history(&self, args: ActionArgs) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::MeasurementHistory {
            action: args.action, expected_revision: args.expected_revision, reply,
        }).await
    }

    async fn cancel_active_action(&self) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::CancelActiveAction { reply }).await
    }

    async fn select_measurement(&self, args: SelectMeasurementArgs) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::SelectMeasurement { capture_id: args.capture_id, id: args.id, reply }).await
    }

    async fn set_calibration(&self, args: SetCalibrationArgs) -> Result<Calibration, String> {
        self.send(|reply| UiRequest::SetCalibration {
            capture_id: args.capture_id, expected_revision: args.expected_revision,
            reference_type: args.reference_type, points: args.points, known_mm: args.known_mm,
            expanded_uncertainty_mm: args.expanded_uncertainty_mm, reply,
        }).await
    }

    async fn list_cameras(&self) -> Result<Vec<CameraView>, String> {
        self.send(|reply| UiRequest::ListCameras { reply }).await
    }

    async fn start_camera(&self, index: usize) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::StartCamera { index, reply }).await
    }

    async fn stream_control(&self, args: StreamControlArgs) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::StreamControl {
            action: args.action, expected_revision: args.expected_revision, reply,
        }).await
    }

    async fn open_image(&self, path: PathBuf) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::OpenImage { path, reply }).await
    }

    async fn open_project(&self, path: PathBuf) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::OpenProject { path, reply }).await
    }

    async fn save_project(&self, path: PathBuf, overwrite: bool) -> Result<String, String> {
        self.send(|reply| UiRequest::SaveProject { path, overwrite, reply }).await
    }

    async fn capture_photo(&self, path: PathBuf, overwrite: bool) -> Result<String, String> {
        self.send(|reply| UiRequest::CapturePhoto { path, overwrite, reply }).await
    }

    async fn set_workspace(&self, args: SetWorkspaceArgs) -> Result<McpContext, String> {
        self.send(|reply| UiRequest::SetWorkspace {
            optical_setup: args.optical_setup, measurement_tool: args.measurement_tool,
            show_grid: args.show_grid, show_center_guides: args.show_center_guides,
            zoom_percent: args.zoom_percent, reply,
        }).await
    }

    async fn prepare_report(&self, folder: Option<PathBuf>) -> Result<ReportPreparation, String> {
        self.send(|reply| UiRequest::PrepareReport { folder, reply }).await
    }

    async fn export_report(&self, args: ExportReportArgs) -> Result<ReportExportResult, String> {
        self.send(|reply| UiRequest::ExportReport { args, reply }).await
    }
}

#[derive(Clone, Serialize)]
pub struct McpContext {
    pub image_available: bool,
    pub image_frozen: bool,
    pub capture_id: String,
    pub document_revision: u64,
    pub session_saved: bool,
    pub stream_running: bool,
    pub stream_paused: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub selected_measurement: Option<u64>,
    pub image_source: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub device: String,
    pub device_id: String,
    pub optical_setup: String,
    pub mm_per_pixel: Option<f64>,
    pub calibration: Option<Calibration>,
    pub image_quality_assessment: Option<crate::quality::ImageQualityAssessment>,
    pub measurement_protocol: Option<crate::storage::MeasurementProtocol>,
    pub annotations: Vec<crate::geometry::Annotation>,
    pub calibration_history: Vec<Calibration>,
    pub available_calibrations: Vec<Calibration>,
    pub coordinate_system: &'static str,
    pub measurements: Vec<MeasurementView>,
}

#[derive(Clone, Serialize)]
pub struct MeasurementView {
    pub id: u64,
    pub is_construction: bool,
    pub source_measurement_ids: Vec<u64>,
    pub dependency_stale: bool,
    pub kind: &'static str,
    pub label: String,
    pub capture_id: String,
    pub calibration_id: Option<String>,
    pub mm_per_pixel: Option<f64>,
    pub mm_per_pixel_x: Option<f64>,
    pub mm_per_pixel_y: Option<f64>,
    pub expanded_uncertainty_mm: Option<f64>,
    pub protocol_id: Option<String>,
    pub protocol_version: Option<u32>,
    pub origin: String,
    pub method: Option<String>,
    pub points_native_px: Vec<Point>,
    pub components: Vec<MeasurementComponent>,
    pub value: String,
}

#[derive(Clone, Serialize)]
pub struct MeasurementComponent {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

impl MeasurementView {
    pub fn from_measurement(measurement: &Measurement) -> Self {
        let kind = match measurement.kind {
            crate::geometry::Tool::Distance => "distance",
            crate::geometry::Tool::Polyline => "polyline",
            crate::geometry::Tool::Angle => "angle",
            crate::geometry::Tool::Circle => "circle",
            crate::geometry::Tool::Rectangle => "rectangle",
            crate::geometry::Tool::Polygon => "polygon",
            crate::geometry::Tool::PointLineDistance => "point_line_distance",
            crate::geometry::Tool::ParallelDistance => "parallel_distance",
            crate::geometry::Tool::CircleFit => "circle_fit",
            crate::geometry::Tool::Ellipse => "ellipse",
            crate::geometry::Tool::Arc => "arc",
            crate::geometry::Tool::Coordinate => "coordinate",
            crate::geometry::Tool::ObjectCount => "object_count",
        };
        Self {
            id: measurement.id,
            is_construction: measurement.is_construction,
            source_measurement_ids: measurement.source_measurement_ids.clone(),
            dependency_stale: measurement.dependency_stale,
            kind,
            label: measurement.label.clone(),
            capture_id: measurement.capture_id.clone(),
            calibration_id: measurement.calibration_id.clone(),
            mm_per_pixel: measurement.mm_per_pixel,
            mm_per_pixel_x: measurement.mm_per_pixel_x,
            mm_per_pixel_y: measurement.mm_per_pixel_y,
            expanded_uncertainty_mm: measurement.expanded_uncertainty_mm,
            protocol_id: measurement.protocol_id.clone(),
            protocol_version: measurement.protocol_version,
            origin: measurement.origin.clone(),
            method: measurement.method.clone(),
            points_native_px: measurement.points.clone(),
            components: if measurement.is_construction || measurement.dependency_stale { Vec::new() } else { measurement_components(measurement) },
            value: crate::geometry::result(measurement),
        }
    }
}

fn measurement_components(measurement: &Measurement) -> Vec<MeasurementComponent> {
    use crate::geometry::{angle_deg, arc_geometry, ellipse_geometry, fit_circle, parallel_line_distance,
        point_line_distance, polygon_area, polyline_length, Tool};
    let metric_points = crate::geometry::metric_points(measurement);
    let calibrated = metric_points.is_some();
    let owned_points = metric_points.unwrap_or_else(|| measurement.points.clone());
    let points = &owned_points;
    let length_unit = if calibrated { "mm" } else { "px" };
    let area_unit = if calibrated { "mm²" } else { "px²" };
    let component = |name: &str, value: f64, unit: &str| MeasurementComponent {
        name: name.into(), value, unit: unit.into(),
    };
    match measurement.kind {
        Tool::Distance if points.len() >= 2 => vec![component("distance", points[0].dist(points[1]), length_unit)],
        Tool::Polyline if points.len() >= 2 => vec![component("length", polyline_length(points), length_unit)],
        Tool::Angle if points.len() >= 3 => angle_deg(points[0], points[1], points[2])
            .map(|value| vec![component("angle", value, "°")]).unwrap_or_default(),
        Tool::Circle if points.len() >= 2 => vec![component("diameter", points[0].dist(points[1]) * 2.0, length_unit)],
        Tool::Rectangle if points.len() >= 2 => {
            let width = (points[0].x - points[1].x).abs();
            let height = (points[0].y - points[1].y).abs();
            vec![
                component("width", width, length_unit),
                component("height", height, length_unit),
                component("area", width * height, area_unit),
            ]
        }
        Tool::Polygon if points.len() >= 3 => vec![component("area", polygon_area(points), area_unit)],
        Tool::PointLineDistance if points.len() >= 3 => point_line_distance(points[0], points[1], points[2])
            .map(|distance| vec![component("perpendicular_distance", distance, length_unit)]).unwrap_or_default(),
        Tool::ParallelDistance if points.len() >= 4 => parallel_line_distance(points[0], points[1], points[2], points[3])
            .map(|(distance, angle)| vec![component("separation", distance, length_unit), component("angular_deviation", angle, "°")]).unwrap_or_default(),
        Tool::CircleFit if points.len() >= 3 => fit_circle(points)
            .map(|circle| vec![component("diameter", circle.radius * 2.0, length_unit),
                component("fit_error_rms", circle.rms_radial_error, length_unit),
                component("fit_error_max", circle.max_radial_error, length_unit)]).unwrap_or_default(),
        Tool::Ellipse if points.len() >= 3 => ellipse_geometry(points)
            .map(|ellipse| vec![component("major_axis", ellipse.semi_major * 2.0, length_unit),
                component("minor_axis", ellipse.semi_minor * 2.0, length_unit),
                component("area", ellipse.area, area_unit)]).unwrap_or_default(),
        Tool::Arc if points.len() >= 3 => arc_geometry(points)
            .map(|arc| vec![component("radius", arc.radius, length_unit),
                component("arc_length", arc.radius * arc.sweep_radians.abs(), length_unit),
                component("sweep", arc.sweep_radians.abs().to_degrees(), "°")]).unwrap_or_default(),
        Tool::Coordinate if !points.is_empty() => vec![
            component("x", points[0].x, length_unit), component("y", points[0].y, length_unit),
        ],
        Tool::ObjectCount if !points.is_empty() => vec![component("count", points.len() as f64, "objects")],
        _ => Vec::new(),
    }
}

pub struct ImagePayload {
    pub context: McpContext,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CropImageArgs {
    /// Capture ID returned by `freeze_active_image`.
    pub capture_id: String,
    /// Left coordinate in the original image, in native pixels.
    pub x: u32,
    /// Top coordinate in the original image, in native pixels.
    pub y: u32,
    /// Crop width in native pixels.
    pub width: u32,
    /// Crop height in native pixels.
    pub height: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NativePoint {
    /// Horizontal coordinate in original-image pixels, origin at upper left.
    pub x: f64,
    /// Vertical coordinate in original-image pixels, origin at upper left.
    pub y: f64,
}

impl From<&NativePoint> for Point {
    fn from(point: &NativePoint) -> Self {
        Point { x: point.x, y: point.y }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MeasurementInput {
    /// Construction references are not measurements or conformity evidence.
    #[serde(default)]
    pub is_construction: bool,
    /// Existing construction/measurement parents; does not validate the child.
    #[serde(default)]
    pub source_measurement_ids: Vec<u64>,
    /// One of: distance, polyline, angle, circle, rectangle, polygon, point_line_distance, parallel_distance, circle_fit, ellipse, arc, coordinate, object_count.
    /// Circle is center then edge; rectangle is opposite corners; point_line_distance is point then two line points;
    /// parallel_distance is two points per line; circle_fit is three or more contour points; ellipse is center plus two conjugate-axis endpoints; arc is start, through-point, end; object_count uses one point per manually counted object.
    pub kind: String,
    /// Native pixel coordinates on the frozen image. Use a distance segment perpendicular to the feature for width.
    pub points: Vec<NativePoint>,
    /// Optional concise reference shown in the measurement list and report.
    pub label: Option<String>,
    /// Optional method or inspection note retained with the measurement and project.
    pub method: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct AddedMeasurement {
    pub id: u64,
    pub is_construction: bool,
    pub source_measurement_ids: Vec<u64>,
    pub dependency_stale: bool,
    pub kind: String,
    pub label: String,
    pub capture_id: String,
    pub calibration_id: Option<String>,
    pub mm_per_pixel: Option<f64>,
    pub mm_per_pixel_x: Option<f64>,
    pub mm_per_pixel_y: Option<f64>,
    pub protocol_id: Option<String>,
    pub protocol_version: Option<u32>,
    pub origin: String,
    pub method: Option<String>,
    pub points_native_px: Vec<Point>,
    pub components: Vec<MeasurementComponent>,
    pub value: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MeasurementUpdateInput {
    /// Existing measurement ID returned by DPER_Measure.
    pub id: u64,
    /// Replace the geometry while preserving its measurement type. Omit to keep the current points.
    pub points: Option<Vec<NativePoint>>,
    /// Replace the label. An empty string restores the generated M{id} label; omit to keep it.
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateMeasurementsArgs {
    pub capture_id: String,
    /// Revision from get_measurement_context; omit only when deliberately accepting concurrent edits.
    pub expected_revision: Option<u64>,
    /// Updates are validated together and either all applied or none.
    pub updates: Vec<MeasurementUpdateInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteMeasurementsArgs {
    pub capture_id: String,
    pub expected_revision: Option<u64>,
    pub ids: Vec<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetCalibrationArgs {
    pub capture_id: String,
    pub expected_revision: Option<u64>,
    /// segment, circle_diameter, or rectangle_diagonal
    pub reference_type: String,
    /// Two native-pixel points. For circle_diameter, point 1 is the centre and point 2 is on the edge.
    pub points: Vec<NativePoint>,
    /// Known physical length in millimetres (circle_diameter means the full diameter).
    pub known_mm: f64,
    /// Optional expanded uncertainty U in millimetres. Omit it when unknown.
    #[serde(default)]
    pub expanded_uncertainty_mm: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct CameraView {
    pub index: usize,
    pub name: String,
    pub device_id: String,
    pub selected: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathArgs {
    /// Absolute or relative local path to an image or DPER project.
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SavePathArgs {
    /// Destination path on the local computer.
    pub path: String,
    /// Replaces existing output only when true.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartCameraArgs {
    /// Zero-based index returned by list_cameras.
    pub index: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ActionArgs {
    /// undo or redo
    pub action: String,
    /// Optional document revision from get_measurement_context.
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StreamControlArgs {
    /// freeze, resume, or stop
    pub action: String,
    /// Optional document revision from get_measurement_context.
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureIdArgs {
    pub capture_id: String,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SelectMeasurementArgs {
    pub capture_id: String,
    /// Measurement ID to select; null clears selection.
    pub id: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetWorkspaceArgs {
    /// Named optical configuration used to match a saved calibration.
    pub optical_setup: Option<String>,
    /// distance, polyline, angle, circle, rectangle, polygon, point_line_distance, parallel_distance, circle_fit, ellipse, arc, coordinate, object_count, or select.
    pub measurement_tool: Option<String>,
    pub show_grid: Option<bool>,
    pub show_center_guides: Option<bool>,
    /// Zoom percentage, within the same range as the image viewer.
    pub zoom_percent: Option<f32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PrepareReportArgs {
    /// Folder to scan non-recursively for PNG/JPEG images; defaults to the active image folder.
    pub folder: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct ReportItemView {
    pub key: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub selected: bool,
    pub measurement_count: usize,
    pub calibration_id: Option<String>,
    pub calibrations: Vec<Calibration>,
    pub measurements: Vec<MeasurementView>,
    pub warning: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct ReportPreparation {
    pub folder: String,
    pub capture_id: String,
    pub document_revision: u64,
    pub items: Vec<ReportItemView>,
    pub selected_count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportPhotoMetadata {
    pub key: String,
    pub description: Option<String>,
    pub include_timestamp: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportReportArgs {
    pub capture_id: String,
    /// Document revision returned by prepare_report.
    pub expected_revision: Option<u64>,
    /// pdf or pptx
    pub format: String,
    /// Destination file path. Companion .dper_measure.json data is written beside it.
    pub destination: String,
    /// Image folder to include; defaults to the active image's folder or DPER_Measure documents folder.
    pub folder: Option<String>,
    /// Exact keys returned by prepare_report. Omit to include all valid images in that folder.
    pub selected_image_keys: Option<Vec<String>>,
    pub title: Option<String>,
    pub include_title: Option<bool>,
    pub include_folder_name: Option<bool>,
    pub include_picture_count: Option<bool>,
    pub include_generation_date: Option<bool>,
    pub include_sequence_numbers: Option<bool>,
    pub include_file_names: Option<bool>,
    pub include_descriptions: Option<bool>,
    pub include_measurements: Option<bool>,
    /// Optional operator, part, drawing revision, lot, inspection note, and approver audit fields.
    pub operator_name: Option<String>,
    pub part_reference: Option<String>,
    pub drawing_revision: Option<String>,
    pub lot_number: Option<String>,
    pub audit_notes: Option<String>,
    pub approval_status: Option<String>,
    pub approved_by: Option<String>,
    pub include_audit_page: Option<bool>,
    pub photo_metadata: Option<Vec<ReportPhotoMetadata>>,
    /// Replaces both report and companion manifest only when true.
    #[serde(default)]
    pub overwrite: bool,
    /// Saves report title, audit fields, descriptions, timestamps, and checkboxes in the source folder preferences.
    #[serde(default)]
    pub persist_preferences: bool,
}

#[derive(Clone, Serialize)]
pub struct ReportExportResult {
    pub format: String,
    pub report_path: String,
    pub manifest_path: String,
    pub manifest_written: bool,
    pub image_count: usize,
    pub warning: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddMeasurementsArgs {
    /// Capture ID returned by `freeze_active_image`; protects against measuring a different frame.
    pub capture_id: String,
    /// Revision from get_measurement_context; omit only when deliberately accepting concurrent edits.
    pub expected_revision: Option<u64>,
    /// Measurements are validated together and either all added or none.
    pub measurements: Vec<MeasurementInput>,
}

#[derive(Clone)]
struct DperMcpServer {
    bridge: Bridge,
}

#[tool_router]
impl DperMcpServer {
    #[tool(description = "Extract bounded vector geometry proposals from one page of the active drawing PDF. Uses the source file owned by the application, verifies its content fingerprint and installs the catalog only at the expected document revision. PDF circles and lines are localization hypotheses, never measured contours or calibration; no nominal dimension is fitted to a primitive.")]
    async fn analyze_drawing_geometry(&self, Parameters(args): Parameters<GeometryPageArgs>) -> Result<CallToolResult, ErrorData> {
        let expected_revision = args.expected_revision;
        let requested_page = args.page;
        let source = match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::Source(args))).await {
            Ok(source) => source,
            Err(error) => return Ok(tool_error(error)),
        };
        let source: DrawingGeometrySource = match serde_json::from_value(source) {
            Ok(source) => source,
            Err(error) => return Ok(tool_error(format!("Source vectorielle de l'application invalide : {error}"))),
        };
        let DrawingGeometrySource { path, document_fingerprint, page } = source;
        if path.trim().is_empty() || document_fingerprint.trim().is_empty() || page != requested_page {
            return Ok(tool_error("La source PDF ou sa page ne correspond plus à la demande. Relisez le plan actif."));
        }
        let catalog = match tokio::task::spawn_blocking(move ||
            crate::dper_pdf_geometry::analyze_pdf_geometry(std::path::Path::new(&path), page)).await
        {
            Ok(Ok(catalog)) => catalog,
            Ok(Err(error)) => return Ok(tool_error(error)),
            Err(error) => return Ok(tool_error(format!("Extraction vectorielle interrompue : {error}"))),
        };
        if catalog.document_fingerprint != document_fingerprint || catalog.geometry.page != requested_page {
            return Ok(tool_error("Le fichier PDF ou sa page a changé depuis l'analyse du plan. Réanalysez la source avant d'installer le catalogue."));
        }
        match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::InstallCatalog {
            catalog, expected_revision,
        })).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "List cached drawing vector proposals on a page, with bounded pagination and optional primitive-kind or exact-ID filtering. Proposals retain their source evidence and ambiguities; they are not photo measurements, verified physical views or nominal-based fits. Analyze the drawing geometry first if no current catalog is available.")]
    async fn get_drawing_geometries(&self, Parameters(args): Parameters<ListGeometryArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::List(args))).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Define an explicit page-local drawing view from selected characteristic and vector primitive IDs. This reviewable scope preserves all selected sources; it does not automatically identify a physical view or confirm dimensions. Replacing a view may invalidate bindings and registration that depend on it.")]
    async fn define_drawing_view(&self, Parameters(args): Parameters<DefineDrawingViewArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::DefineView(args))).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Return reviewable text-to-vector binding hypotheses for a characteristic in an explicit drawing view. All compatible circular alternatives are retained; proximity ranks candidates without confirming them or fitting nominal sizes. Only circular radius/diameter localization is currently supported, not arbitrary lines, arcs or CAD relations.")]
    async fn get_drawing_binding_candidates(&self, Parameters(args): Parameters<BindingCandidatesArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::Candidates(args))).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Select a known drawing binding candidate with an explicit source justification. This records an interpretation hypothesis, never a measured contour or human review. Selection cannot mark a binding reviewed; review belongs to the current-session UI. The nominal is never used to force the chosen circle or physical calibration.")]
    async fn select_drawing_binding(&self, Parameters(args): Parameters<SelectBindingArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::Select(args))).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Inspect a drawing feature through its selected circular binding and view-scoped registration on the frozen image. Search geometry comes from current bound PDF evidence; no coordinate or reviewed inputs are accepted. Actual image edges determine the observed circle, never its nominal size. False previews and true commits retain provenance; unreviewed, changed or imported bindings block validation, and physical qualification plus total measurement uncertainty are still required for conformity. Four-stop clearance circles and endpoint measurements are not supported.")]
    async fn inspect_bound_feature(&self, Parameters(args): Parameters<InspectBoundFeatureArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Drawing(DrawingRequest::InspectBound(args))).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Read the structured drawing control plan, assisted registration, grouped contour observations and freshly reassessed validation evidence. Software contour validation and conformity are separate; imported or changed geometry requires fresh inspection.")]
    async fn get_inspection_state(&self) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::GetState).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Propose a traceable correction to a drawing feature type or unit after reading the actual drawing. Preserves original text, nominal and tolerance, records the source justification, clears human confirmation and invalidates registration. This changes interpretation only, never observed pixels or calibration.")]
    async fn propose_drawing_interpretation(&self, Parameters(args): Parameters<ProposeInterpretationArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::ProposeInterpretation(args)).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Extract a structured control plan from a local PDF or drawing image, preserving text/OCR sources, units, tolerances, repetitions and 2D limitations. This conservative text parser does not reconstruct arbitrary drawing views or find photo features. Uncertain candidates remain unconfirmed.")]
    async fn analyze_drawing(&self, Parameters(args): Parameters<AnalyzeDrawingArgs>) -> Result<CallToolResult, ErrorData> {
        let context = self.bridge.get_context().await.map_err(internal_error)?;
        if context.document_revision != args.expected_revision {
            return Ok(tool_error("Le document a changé. Relisez le contexte."));
        }
        let expected_revision = args.expected_revision;
        let plan = match tokio::task::spawn_blocking(move || crate::dper_drawing::analyze_path(std::path::Path::new(&args.path))).await {
            Ok(Ok(plan)) => plan,
            Ok(Err(error)) => return Ok(tool_error(error)),
            Err(error) => return Ok(tool_error(format!("Analyse interrompue : {error}"))),
        };
        match self.bridge.inspection(InspectionRequest::InstallPlan { plan, expected_revision }).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Estimate a robust similarity from explicit drawing-to-photo landmarks. Handles translation, rotation and scale for search localization; never replaces physical calibration. Automatic correspondence discovery is not implemented in this first increment.")]
    async fn register_inspection(&self, Parameters(args): Parameters<RegisterInspectionArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Register(args)).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Inspect an identified drawing feature on the frozen calibrated image using directional subpixel profiles and a robust line/circle/arc fit. Supply search geometry in the registered drawing frame. Records observed contour evidence in a named group. Line endpoints and arc extents are not measured; physical qualification and total measurement uncertainty remain required for conformity. This does not measure a four-stop clearance circle from a circular contour model.")]
    async fn inspect_feature(&self, Parameters(args): Parameters<InspectFeatureArgs>) -> Result<CallToolResult, ErrorData> {
        match self.bridge.inspection(InspectionRequest::Inspect(args)).await {
            Ok(state) => json_result(&state), Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Read the active DPER_Measure image, active and historical calibration profiles, all saved calibration profiles available for the device/configuration, and all measurements. Results include native points, numeric components, units, capture/calibration provenance, origin, and method. Without calibration, measurements remain in pixels.")]
    async fn get_measurement_context(&self) -> Result<String, ErrorData> {
        let context = self.bridge.get_context().await.map_err(internal_error)?;
        serde_json::to_string_pretty(&context).map_err(|error| internal_error(error.to_string()))
    }

    #[tool(description = "Freeze the currently displayed microscope frame (if live) and return the unannotated source image at native resolution with its capture ID and calibration metadata. Call this before selecting measurement coordinates; the returned ID must be supplied to later crop and add tools.")]
    async fn freeze_active_image(&self) -> Result<CallToolResult, ErrorData> {
        let payload = match self.bridge.freeze_image().await {
            Ok(payload) => payload,
            Err(error) => return Ok(tool_error(error)),
        };
        image_result(payload)
    }

    #[tool(description = "Return a full-resolution crop of any size from the frozen image. Coordinates are in original-image pixels and must use the capture ID from freeze_active_image. Use this to inspect a feature closely before placing endpoints.")]
    async fn get_image_crop(
        &self,
        Parameters(args): Parameters<CropImageArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        if args.width == 0 || args.height == 0 { return Ok(tool_error("La largeur et la hauteur du recadrage doivent être supérieures à zéro.")); }
        let payload = match self.bridge.get_crop(args).await {
            Ok(payload) => payload,
            Err(error) => return Ok(tool_error(error)),
        };
        image_result(payload)
    }

    #[tool(description = "Add one or more geometry measurements to the frozen image in native source-pixel coordinates. For object_count, supply one point for each manually counted object; this does not detect objects automatically. For a lead width, place a distance segment from one visible edge to the opposite edge, perpendicular to the lead; use a short label such as LM259 patte 1. The image must first be frozen, pass the configured provisional image-quality screen, and its capture ID supplied. Calibration is taken from DPER_Measure; uncalibrated dimensional results stay in pixels. The app checks bounds and geometry and shows the additions for review. Passing provisional thresholds is not metrology qualification.")]
    async fn add_measurements(
        &self,
        Parameters(args): Parameters<AddMeasurementsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        if args.measurements.is_empty() { return Ok(tool_error("Fournissez au moins une mesure.")); }
        let added = match self.bridge.add_measurements(args).await {
            Ok(added) => added,
            Err(error) => return Ok(tool_error(error)),
        };
        let json = serde_json::to_string_pretty(&added)
            .map_err(|error| internal_error(error.to_string()))?;
        Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
    }

    #[tool(description = "Calibrate the active frozen image from two native-pixel reference points. reference_type is segment, circle_diameter, or rectangle_diagonal; known_mm is the real length (for a circle, its full diameter). Optionally provide expanded_uncertainty_mm (U) in millimetres; omit it when U is unknown. The profile is saved in DPER_Measure using the current device, resolution, and optical setup. Supply the current capture ID and, preferably, document revision from get_measurement_context.")]
    async fn set_calibration(
        &self,
        Parameters(args): Parameters<SetCalibrationArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.set_calibration(args).await {
            Ok(calibration) => json_result(&calibration),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Edit points and/or labels of existing measurements. The measurement type is preserved, points remain in native image pixels, and every requested edit is validated atomically. Use the capture ID and document revision from get_measurement_context to prevent applying edits to stale state.")]
    async fn update_measurements(
        &self,
        Parameters(args): Parameters<UpdateMeasurementsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.update_measurements(args).await {
            Ok(measurements) => json_result(&measurements),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Delete the explicitly listed measurement IDs from the active capture. Deletions enter DPER_Measure undo history and can be restored with measurement_history(action=redo after undo).")]
    async fn delete_measurements(
        &self,
        Parameters(args): Parameters<DeleteMeasurementsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.delete_measurements(args).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Delete every measurement from the specified active capture. This action is recorded in undo history.")]
    async fn clear_measurements(
        &self,
        Parameters(args): Parameters<CaptureIdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.clear_measurements(args).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Use DPER_Measure's measurement history. action is undo or redo; returns the updated context and measurement list.")]
    async fn measurement_history(
        &self,
        Parameters(args): Parameters<ActionArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.measurement_history(args).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Select or deselect a measurement in the DPER_Measure results panel. This changes only the UI selection, not the measurement record.")]
    async fn select_measurement(
        &self,
        Parameters(args): Parameters<SelectMeasurementArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.select_measurement(args).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Cancel an unfinished drawing or calibration, revert an active point edit, or close the report dialog. Equivalent to Escape/Cancel in the interface, and lets MCP take control of the next action.")]
    async fn cancel_active_action(&self) -> Result<CallToolResult, ErrorData> {
        match self.bridge.cancel_active_action().await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "List the microscope cameras detected by DPER_Measure. Use the returned zero-based index with start_camera.")]
    async fn list_cameras(&self) -> Result<CallToolResult, ErrorData> {
        match self.bridge.list_cameras().await {
            Ok(cameras) => json_result(&cameras),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Start a detected microscope camera by its zero-based list_cameras index. Existing unsaved work or an unfinished trace is preserved and blocks replacing the current capture.")]
    async fn start_camera(
        &self,
        Parameters(args): Parameters<StartCameraArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.start_camera(args.index).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Control the microscope stream with action freeze, resume, or stop. Freeze creates an immutable measurement capture; resume starts a new capture and requires the current session to be saved; stop leaves the last frame available. Pass the document revision from get_measurement_context when available.")]
    async fn control_stream(
        &self,
        Parameters(args): Parameters<StreamControlArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.stream_control(args).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Open a local PNG, JPEG, BMP, or TIFF image in DPER_Measure. The path must be explicit. Unsaved session data or a pending trace prevents replacing the current image.")]
    async fn open_image(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.open_image(PathBuf::from(args.path)).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Open a local .dperproj project or compatible session JSON in DPER_Measure, restoring its source image, calibration profiles, and measurements.")]
    async fn open_project(
        &self,
        Parameters(args): Parameters<PathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.open_project(PathBuf::from(args.path)).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Save the active image, calibration provenance, and measurements as a DPER_Measure project at an explicit local path. Existing output is not replaced unless overwrite=true.")]
    async fn save_project(
        &self,
        Parameters(args): Parameters<SavePathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.save_project(PathBuf::from(args.path), args.overwrite).await {
            Ok(path) => Ok(CallToolResult::success(vec![ContentBlock::text(path)])),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Save the active microscope image as a PNG at an explicit local path. Existing output is not replaced unless overwrite=true.")]
    async fn capture_photo(
        &self,
        Parameters(args): Parameters<SavePathArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.capture_photo(PathBuf::from(args.path), args.overwrite).await {
            Ok(path) => Ok(CallToolResult::success(vec![ContentBlock::text(path)])),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Change DPER_Measure workspace settings available in the UI: named optical setup, measurement tool, alignment grid, centre guides, and zoom. Omit fields to leave them unchanged.")]
    async fn set_workspace(
        &self,
        Parameters(args): Parameters<SetWorkspaceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.set_workspace(args).await {
            Ok(context) => json_result(&context),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Inspect every report input used by DPER_Measure's native PDF/PowerPoint exporter. Returns each image's stable selection key, dimensions, selection state, complete linked measurements and calibration profiles, and any load warning. Folder scanning is non-recursive and defaults to the active image folder.")]
    async fn prepare_report(
        &self,
        Parameters(args): Parameters<PrepareReportArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.prepare_report(args.folder.map(PathBuf::from)).await {
            Ok(preparation) => json_result(&preparation),
            Err(error) => Ok(tool_error(error)),
        }
    }

    #[tool(description = "Export through DPER_Measure's native folder-report generator as PDF or PowerPoint, including image annotations, measurement table, and a companion .dper_measure.json manifest. Use prepare_report first, pass its exact image keys, and provide an explicit destination. Existing files are retained unless overwrite=true. The report is informational and does not itself certify conformity.")]
    async fn export_report(
        &self,
        Parameters(args): Parameters<ExportReportArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.bridge.export_report(args).await {
            Ok(result) => json_result(&result),
            Err(error) => Ok(tool_error(error)),
        }
    }
}

#[tool_handler(
    name = "DPER_Measure",
    version = "0.2.0",
    instructions = "Full local control of DPER_Measure's application functions: camera stream, image and project files, calibration, geometry creation/edit/delete/history, workspace view, and native PDF/PowerPoint reports. Coordinates are native source-image pixels with origin at top-left. Use the capture ID and document revision to target the intended source; the app owns measurements and applies the same geometry, image-quality gate, and calibration rules as its interface. Measurement, uncertainty, and calibration provenance are returned and saved. Image-quality thresholds remain provisional; missing uncertainty or incompatible batch protocols suspend conformity conclusions. Reports do not constitute certified acceptance."
)]
impl rmcp::ServerHandler for DperMcpServer {}

fn image_result(payload: ImagePayload) -> Result<CallToolResult, ErrorData> {
    let expected_len = (payload.width as usize)
        .checked_mul(payload.height as usize)
        .and_then(|pixels| pixels.checked_mul(3));
    if expected_len != Some(payload.rgb.len()) {
        return Ok(tool_error("Les données RGB de l'image active sont incohérentes."));
    }
    let image = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
        payload.width,
        payload.height,
        payload.rgb,
    ).ok_or_else(|| internal_error("Impossible de construire le PNG de l'image."))?;
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| internal_error(format!("Encodage PNG impossible : {error}")))?;

    let context = payload.context;
    let calibration = context.mm_per_pixel
        .map(|scale| format!("Étalonnage : {scale:.8} mm/px."))
        .unwrap_or_else(|| "Image non étalonnée : toute mesure doit rester exprimée en pixels.".into());
    let info = format!(
        "Image source non annotée, capture {}. Dimensions du bloc : {} × {} px. Ce bloc commence au pixel natif ({}, {}) de l'image originale (origine en haut à gauche); reportez ce décalage aux points envoyés à add_measurements. Appareil : {}. Configuration optique : {}. {}",
        context.capture_id, payload.width, payload.height, payload.x, payload.y,
        context.device, context.optical_setup, calibration
    );
    let encoded = STANDARD.encode(output.into_inner());
    Ok(CallToolResult::success(vec![
        ContentBlock::text(info),
        ContentBlock::image(encoded, "image/png"),
    ]))
}

fn tool_error(message: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

fn json_result(value: &impl Serialize) -> Result<CallToolResult, ErrorData> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| internal_error(error.to_string()))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
}

fn internal_error(message: impl Into<String>) -> ErrorData {
    ErrorData::internal_error(message.into(), None)
}

pub fn start(bridge: Bridge) -> Result<ServerHandle, String> {
    let port = parse_mcp_port(std::env::var("DPER_MEASURE_MCP_PORT").ok().as_deref())?;
    let bind_address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let status = Arc::new(Mutex::new(ServerStatus::Starting));
    let thread_status = status.clone();
    let thread_repaint = bridge.repaint.clone();
    let (stop_tx, stop_rx) = oneshot::channel();
    std::thread::Builder::new()
        .name("dper-measure-mcp".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    set_status(&thread_status, &thread_repaint, ServerStatus::Failed(format!("Runtime MCP impossible : {error}")));
                    return;
                }
            };

            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::bind(bind_address).await {
                    Ok(listener) => listener,
                    Err(error) => {
                        set_status(&thread_status, &thread_repaint, ServerStatus::Failed(format!("Port MCP indisponible ({bind_address}) : {error}")));
                        return;
                    }
                };
                let service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
                    {
                        let bridge = bridge.clone();
                        move || Ok(DperMcpServer { bridge: bridge.clone() })
                    },
                    rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default().into(),
                    rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default(),
                );
                let router = Router::new()
                    .nest_service("/mcp", service)
                    .layer(middleware::from_fn_with_state(port, local_origin_guard));
                set_status(&thread_status, &thread_repaint, ServerStatus::Running);
                let server = axum::serve(listener, router).with_graceful_shutdown(async move {
                    let _ = stop_rx.await;
                });
                match server.await {
                    Ok(()) => set_status(&thread_status, &thread_repaint, ServerStatus::Stopped),
                    Err(error) => set_status(&thread_status, &thread_repaint, ServerStatus::Failed(format!("Serveur MCP arrêté : {error}"))),
                }
            });
        })
        .map_err(|error| format!("Démarrage du thread MCP impossible : {error}"))?;

    Ok(ServerHandle { stop: Some(stop_tx), status })
}

fn set_status(status: &Arc<Mutex<ServerStatus>>, repaint: &egui::Context, value: ServerStatus) {
    if let Ok(mut status) = status.lock() {
        *status = value;
    }
    repaint.request_repaint();
}

fn local_request_allowed(host: Option<&str>, origin: Option<&str>, port: u16) -> bool {
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let host_allowed = host.is_some_and(|host| hosts.iter().any(|allowed| allowed == host));
    let origin_allowed = origin.is_none_or(|origin| {
        origin.parse::<axum::http::Uri>().ok().is_some_and(|uri| {
            uri.scheme_str() == Some("http")
                && matches!(uri.host(), Some("127.0.0.1" | "localhost"))
                && uri.port_u16().is_some()
                && uri.path() == "/" && uri.query().is_none()
        })
    });
    host_allowed && origin_allowed
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;

    #[test]
    fn alternate_port_keeps_loopback_host_and_origin_restrictions() {
        assert_eq!(parse_mcp_port(None).unwrap(), 8765);
        assert_eq!(parse_mcp_port(Some("18765")).unwrap(), 18765);
        for invalid in ["0", "65536", "", "127.0.0.1:8765", "-1"] {
            assert!(parse_mcp_port(Some(invalid)).is_err());
        }
        assert!(local_request_allowed(Some("127.0.0.1:18765"), None, 18765));
        assert!(local_request_allowed(Some("localhost:18765"), Some("http://localhost:3000"), 18765));
        assert!(!local_request_allowed(Some("127.0.0.1:8765"), None, 18765));
        for origin in ["http://localhost.evil.test:3000", "http://127.0.0.1:3000/other", "https://localhost:3000", "null", "http://localhost:3000.evil.test"] {
            assert!(!local_request_allowed(Some("localhost:18765"), Some(origin), 18765), "{origin}");
        }
    }
}

async fn local_origin_guard(State(port): State<u16>, request: Request, next: Next) -> Response {
    if !local_request_allowed(request.headers().get("host").and_then(|value| value.to_str().ok()),
        request.headers().get("origin").and_then(|value| value.to_str().ok()), port) {
        return (StatusCode::FORBIDDEN, "DPER_Measure MCP accepts local connections only.").into_response();
    }
    next.run(request).await
}

pub fn build_context(
    image: Option<(&str, u32, u32)>,
    capture_id: &str,
    document_revision: u64,
    session_saved: bool,
    stream_running: bool,
    stream_paused: bool,
    can_undo: bool,
    can_redo: bool,
    selected_measurement: Option<u64>,
    device: &str,
    device_id: &str,
    optical_setup: &str,
    image_frozen: bool,
    mm_per_pixel: Option<f64>,
    calibration: Option<Calibration>,
    calibration_history: &[Calibration],
    available_calibrations: &[Calibration],
    measurements: &[Measurement],
    image_quality_assessment: Option<crate::quality::ImageQualityAssessment>,
    measurement_protocol: Option<crate::storage::MeasurementProtocol>,
    annotations: &[crate::geometry::Annotation],
) -> McpContext {
    let (image_source, image_width, image_height) = match image {
        Some((source, width, height)) => (Some(source.to_string()), Some(width), Some(height)),
        None => (None, None, None),
    };
    McpContext {
        image_available: image_width.is_some(),
        image_frozen,
        capture_id: capture_id.to_string(),
        document_revision,
        session_saved,
        stream_running,
        stream_paused,
        can_undo,
        can_redo,
        selected_measurement,
        image_source,
        image_width,
        image_height,
        device: device.to_string(),
        device_id: device_id.to_string(),
        optical_setup: optical_setup.to_string(),
        mm_per_pixel: mm_per_pixel.filter(|value| value.is_finite() && *value > 0.0),
        calibration,
        image_quality_assessment,
        measurement_protocol,
        annotations: annotations.to_vec(),
        calibration_history: calibration_history.to_vec(),
        available_calibrations: available_calibrations.to_vec(),
        coordinate_system: "pixels natifs de l'image source, origine (0,0) en haut à gauche; x vers la droite, y vers le bas",
        measurements: measurements.iter().map(MeasurementView::from_measurement).collect(),
    }
}

pub fn crop_rgb(
    rgb: &[u8],
    image_width: u32,
    image_height: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 { return Err("La largeur et la hauteur du recadrage doivent être supérieures à zéro.".into()); }
    let right = x.checked_add(width).ok_or("Coordonnée de zone invalide")?;
    let bottom = y.checked_add(height).ok_or("Coordonnée de zone invalide")?;
    if right > image_width || bottom > image_height {
        return Err(format!("La zone demandée sort de l'image source ({image_width} × {image_height} px)."));
    }
    let expected = (image_width as usize)
        .checked_mul(image_height as usize)
        .and_then(|pixels| pixels.checked_mul(3));
    if expected != Some(rgb.len()) {
        return Err("Les données RGB de l'image active sont incohérentes.".into());
    }
    let capacity = (width as usize).checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| "Dimensions de recadrage trop grandes pour cette plateforme.".to_string())?;
    let mut crop = Vec::new();
    crop.try_reserve_exact(capacity)
        .map_err(|error| format!("Mémoire insuffisante pour le recadrage : {error}"))?;
    for row in y..bottom {
        let start = (row as usize * image_width as usize + x as usize) * 3;
        let end = start + width as usize * 3;
        crop.extend_from_slice(&rgb[start..end]);
    }
    Ok(crop)
}


