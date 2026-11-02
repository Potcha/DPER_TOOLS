// Included in exports::tests, reusing the independent export session fixture.
fn evidence_record_for_export(measurement: &Measurement) -> crate::dper_inspection::InspectionRecord {
    serde_json::from_value(serde_json::json!({
        "feature_id":"diameter-A", "group":"Groupe; témoin", "capture_id":measurement.capture_id,
        "source_fingerprint":42, "calibration_id":null,
        "result":{"geometry":{"Circle":{"center":{"x":10.0,"y":10.0},"radius":4.0}},
            "evidence":{"accepted_points":[{"x":14.0,"y":10.0}],"rejected_points":[],"sample_count":96,
                "rms_residual_px":0.125,"max_residual_px":0.25,"coverage_fraction":0.9,"endpoint_extent_verified":false}},
        "value_mm":0.8, "outcome":{"geometry_state":"validated","conformity_status":"indeterminate",
            "blocking_reasons":["line_endpoints_unverified"]}, "measurement_id":measurement.id,
        "measurement_points":measurement.points, "vision_options":crate::dper_vision::VisionOptions {
            maximum_rms_px:0.35, search_radius_px:6.0, ..Default::default() }
    })).unwrap()
}

#[test]
fn csv_xlsx_inspection_metadata_is_historical_source_joined_and_escaped() {
    let mut session = test_session(true);
    session.measurements[0].origin = "vision".into();
    session.measurements[0].source_measurement_ids = vec![7,8];
    let record = evidence_record_for_export(&session.measurements[0]);
    let mut foreign = record.clone();
    foreign.capture_id = "foreign-capture".into(); foreign.group = "WRONG_CAPTURE_GROUP".into();
    session.automated_inspection = Some(crate::dper_inspection::InspectionWorkspace {
        records:vec![record,foreign], ..Default::default()
    });
    let csv=String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
    assert!(csv.lines().next().unwrap().ends_with("dependances;motifs_suspension"));
    assert!(csv.contains("\"Groupe; témoin\""));
    assert!(!csv.contains("WRONG_CAPTURE_GROUP"));
    for expected in ["historique : vérifiée par logiciel","Recherche 6.000 px","RMS max 0.350 px",
        "M7, M8","extrémités réelles non mesurées","conformité suspendue"] { assert!(csv.contains(expected),"{expected}"); }
    let xlsx=measurements_xlsx(&session).unwrap();
    let xml=String::from_utf8_lossy(&xlsx);
    for expected in ["Groupe inspection","Groupe; témoin","historique : vérifiée par logiciel",
        "RMS max 0.350 px","M7, M8","extrémités réelles non mesurées"] { assert!(xml.contains(expected),"{expected}"); }
}

#[test]
fn legacy_inspection_metadata_never_invents_thresholds_and_stale_values_stay_omitted() {
    let mut session=test_session(true);
    let mut record=evidence_record_for_export(&session.measurements[0]);
    let mut value=serde_json::to_value(&record).unwrap();
    value.as_object_mut().unwrap().remove("vision_options");
    record=serde_json::from_value(value).unwrap();
    assert!(record.vision_options.is_none());
    session.automated_inspection=Some(crate::dper_inspection::InspectionWorkspace { records:vec![record], ..Default::default() });
    let csv=String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
    assert!(csv.contains("seuils non conservés"));
    session.measurements[0].dependency_stale=true;
    session.measurements[0].label="STALE_NUMERIC_VALUE".into();
    let csv=String::from_utf8(measurements_csv(&session).unwrap()).unwrap();
    assert_eq!(csv.lines().count(),1);
    let xlsx=measurements_xlsx(&session).unwrap();
    assert!(!String::from_utf8_lossy(&xlsx).contains("STALE_NUMERIC_VALUE"));
}

#[test]
fn supplied_decision_basis_survives_all_exports_as_history_without_granting_conformity() {
    let mut photo = crate::report::test_support::historical_vision_photo();
    let mut json = serde_json::to_value(evidence_record_for_export(&photo.session.measurements[0])).unwrap();
    json["decision_evidence"] = serde_json::json!({
        "basis":{"qualification_reference":"QUAL-DPM16", "qualification_scope":"synthetic fixture only",
            "expanded_uncertainty_mm":0.02, "uncertainty_source":"U-TOTAL-DPM16", "reviewer":"Test operator"},
        "capture_id":photo.session.capture_id, "source_fingerprint":42, "calibration_fingerprint":43,
        "feature_fingerprint":"synthetic", "plan_source_fingerprint":44, "localization_fingerprint":45,
        "measurement_id":photo.session.measurements[0].id, "measurement_revision":0,
        "basis_fingerprint":46, "reviewed":true
    });
    let record: crate::dper_inspection::InspectionRecord = serde_json::from_value(json).unwrap();
    assert!(!record.decision_evidence.as_ref().unwrap().reviewed);
    photo.session.automated_inspection = Some(crate::dper_inspection::InspectionWorkspace {
        records:vec![record], ..Default::default()
    });
    let csv = String::from_utf8(measurements_csv(&photo.session).unwrap()).unwrap();
    let xlsx = measurements_xlsx(&photo.session).unwrap();
    let notes = crate::report::inspection_export_notes(&photo.session).join("\n");
    let xlsx_text = String::from_utf8_lossy(&xlsx);
    for text in [csv.as_str(), xlsx_text.as_ref(), notes.as_str()] {
        for marker in ["QUAL-DPM16", "U-TOTAL-DPM16", "0.020000", "conformité suspendue"] {
            assert!(text.contains(marker), "{marker}");
        }
    }
    let options = crate::report::test_support::dense_options();
    assert!(!options.include_audit_page);
    let dir = std::env::temp_dir().join(crate::new_id("decision-export-test"));
    std::fs::create_dir(&dir).unwrap();
    let pdf = dir.join("decision.pdf"); let pptx = dir.join("decision.pptx");
    crate::report::write_folder_pdf(&pdf, &options, std::slice::from_ref(&photo)).unwrap();
    crate::pptx::write_folder_pptx(&pptx, &options, &[photo]).unwrap();
    for path in [&pdf, &pptx] {
        let bytes = std::fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for marker in ["QUAL-DPM16", "U-TOTAL-DPM16", "0.020000"] { assert!(text.contains(marker), "{marker}"); }
        assert!(!text.contains(">Dans la tolérance<"));
    }
    if let Ok(folder) = std::env::var("DPER_TEST_DECISION_OUTPUT") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for path in [&pdf, &pptx] { std::fs::copy(path, folder.join(path.file_name().unwrap())).unwrap(); }
    }
    std::fs::remove_file(pdf).unwrap(); std::fs::remove_file(pptx).unwrap(); std::fs::remove_dir(dir).unwrap();
}
