// Included beneath dper_inspection::tests to reuse actual observed-contour fixtures.
fn basis() -> DecisionBasis {
    DecisionBasis {
        qualification_reference: "Independent certificate Q-17".into(),
        qualification_scope: "Current independent optical setup and measurement plane; diameter/radius in mm".into(),
        expanded_uncertainty_mm: 0.02,
        uncertainty_source: "Established total expanded U for this characteristic; report U-17, k=2".into(),
        reviewer: "Test operator".into(),
    }
}

fn committed(text: &str) -> (RgbImage, InspectionWorkspace, FrameContext, InspectionRecord, Vec<Measurement>) {
    let image = disk();
    let mut workspace = workspace(&image, text, 1.0);
    let frame = frame();
    let mut record = circle(&workspace, &image, &frame, 1.0);
    record.measurement_id = Some(1);
    record.measurement_revision = Some(0);
    let measured = measurement(&record);
    workspace.records.push(record.clone());
    (image, workspace, frame, record, vec![measured])
}

fn reviewed() -> (RgbImage, InspectionWorkspace, FrameContext, InspectionRecord, Vec<Measurement>) {
    let (image, workspace, frame, mut record, measured) = committed("Ø5 ±0.2 mm");
    attach_decision_evidence(&mut record, &workspace, &image, &frame, &measured, basis()).unwrap();
    assert_eq!(record.outcome.geometry_state, GeometryState::Validated);
    assert_eq!(record.outcome.conformity_status, ConformityStatus::Conforming);
    (image, workspace, frame, record, measured)
}

#[test]
fn documented_current_decision_uses_full_interval_and_inclusive_tolerance_bounds() {
    for (lower_offset, upper_offset, expected) in [
        (-0.1, 0.1, ConformityStatus::Conforming),
        (-0.02, 0.02, ConformityStatus::Conforming),
        (0.02, 0.1, ConformityStatus::Indeterminate),
        (-0.1, -0.02, ConformityStatus::Indeterminate),
        (0.03, 0.1, ConformityStatus::Nonconforming),
        (-0.1, -0.03, ConformityStatus::Nonconforming),
        (-0.01, 0.1, ConformityStatus::Indeterminate),
    ] {
        let (image, mut workspace, frame, mut record, measured) = committed("Ø5 ±0.2 mm");
        let value = record.value_mm.unwrap();
        let tolerance = workspace.plan.as_mut().unwrap().characteristics[0].tolerance.as_mut().unwrap();
        tolerance.lower_limit = value + lower_offset;
        tolerance.upper_limit = value + upper_offset;
        attach_decision_evidence(&mut record, &workspace, &image, &frame, &measured, basis()).unwrap();
        assert_eq!(record.outcome.conformity_status, expected);
        assert_eq!(record.decision_evidence.as_ref().unwrap().basis.expanded_uncertainty_mm, 0.02);
    }
}

#[test]
fn old_profile_passed_or_verified_and_profile_u_never_supply_decision_authority() {
    for status in ["passed", "verified"] {
        let (image, workspace, mut frame, mut record, measured) = committed("Ø5 ±0.2 mm");
        frame.calibration.as_mut().unwrap().validation_status = status.into();
        record.calibration_fingerprint = calibration_fingerprint(frame.calibration.as_ref());
        let outcome = assess_record(&record, &workspace, &image, &frame, &measured);
        assert_eq!(outcome.geometry_state, GeometryState::Validated);
        assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
        assert!(outcome.blocking_reasons.contains(&"calibration_unqualified".into()));
        assert!(outcome.blocking_reasons.contains(&"uncertainty_unknown".into()));
    }
}

#[test]
fn tolerance_missing_or_unconfirmed_never_produces_a_physical_verdict() {
    let (image, workspace, frame, record, measured) = reviewed();
    for unconfirmed in [false, true] {
        let mut changed = workspace.clone();
        let feature = &mut changed.plan.as_mut().unwrap().characteristics[0];
        if unconfirmed { feature.confirmed = false; } else { feature.tolerance = None; }
        let outcome = assess_record(&record, &changed, &image, &frame, &measured);
        assert_eq!(outcome.conformity_status, ConformityStatus::NotApplicable);
        let mut record = record.clone();
        assert!(attach_decision_evidence(&mut record, &changed, &image, &frame, &measured, basis()).is_err());
    }
    let mut changed = workspace;
    changed.plan.as_mut().unwrap().characteristics[0].unit = None;
    let outcome = assess_record(&record, &changed, &image, &frame, &measured);
    assert_eq!(outcome.conformity_status, ConformityStatus::NotApplicable);
    assert!(outcome.blocking_reasons.contains(&"tolerance_unit_or_limits_unresolved".into()));
}

#[test]
fn unreviewed_or_modified_basis_blocks_physical_decision_without_erasing_contour_validation() {
    let (image, workspace, frame, record, measured) = reviewed();
    for change in 0..4 {
        let mut changed = record.clone();
        let evidence = changed.decision_evidence.as_mut().unwrap();
        match change {
            0 => evidence.reviewed = false,
            1 => evidence.basis.expanded_uncertainty_mm = 0.001,
            2 => evidence.basis.qualification_scope.push_str(" altered"),
            _ => evidence.basis.expanded_uncertainty_mm = f64::NAN,
        }
        let outcome = assess_record(&changed, &workspace, &image, &frame, &measured);
        assert_eq!(outcome.geometry_state, GeometryState::Validated);
        assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
        assert!(outcome.blocking_reasons.iter().any(|reason| reason.starts_with("decision_evidence_")));
    }
}

#[test]
fn imported_claims_cannot_restore_review_or_receive_new_review_without_observation() {
    let (image, workspace, frame, record, measured) = reviewed();
    let mut json = serde_json::to_value(&record).unwrap();
    json["imported"] = false.into();
    json["decision_evidence"]["reviewed"] = true.into();
    let mut restored: InspectionRecord = serde_json::from_value(json).unwrap();
    assert!(restored.imported);
    assert!(!restored.decision_evidence.as_ref().unwrap().reviewed);
    assert_eq!(assess_record(&restored, &workspace, &image, &frame, &measured).conformity_status,
        ConformityStatus::Indeterminate);
    assert!(attach_decision_evidence(&mut restored, &workspace, &image, &frame, &measured, basis()).is_err());
}

#[test]
fn current_physical_scope_changes_suspend_reviewed_decision() {
    let (image, workspace, frame, record, measured) = reviewed();
    let mut altered_pixels = image.clone();
    altered_pixels.put_pixel(0, 0, Rgb([1, 2, 3]));
    assert_eq!(assess_record(&record, &workspace, &altered_pixels, &frame, &measured).conformity_status,
        ConformityStatus::Indeterminate);
    let mut changed_frame = frame.clone();
    changed_frame.calibration.as_mut().unwrap().author = "Different profile under same ID".into();
    assert_eq!(assess_record(&record, &workspace, &image, &changed_frame, &measured).conformity_status,
        ConformityStatus::Indeterminate);
    let mut changed_plan = workspace.clone();
    changed_plan.plan.as_mut().unwrap().characteristics[0].tolerance.as_mut().unwrap().upper_limit += 0.1;
    let outcome = assess_record(&record, &changed_plan, &image, &frame, &measured);
    assert_eq!(outcome.geometry_state, GeometryState::Validated);
    assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
    assert!(outcome.blocking_reasons.contains(&"decision_evidence_changed".into()));
    let mut changed_generation = workspace;
    changed_generation.dependencies.invalidate(&measured, &[1]);
    assert_eq!(assess_record(&record, &changed_generation, &image, &frame, &measured).conformity_status,
        ConformityStatus::Indeterminate);
    let mut stale_native = measured;
    stale_native[0].dependency_stale = true;
    let mut record = record;
    assert!(attach_decision_evidence(&mut record, &changed_generation, &image, &frame, &stale_native, basis()).is_err());
}

#[test]
fn review_rejects_preview_bad_documentation_invalid_u_and_ambiguous_native_ids() {
    let (image, workspace, frame, record, measured) = committed("Ø5 ±0.2 mm");
    let mut preview = record.clone();
    preview.measurement_id = None;
    assert!(attach_decision_evidence(&mut preview, &workspace, &image, &frame, &measured, basis()).is_err());
    for invalid in [0.0, -0.1, f64::NAN, f64::INFINITY] {
        let mut supplied = basis(); supplied.expanded_uncertainty_mm = invalid;
        assert!(attach_decision_evidence(&mut record.clone(), &workspace, &image, &frame, &measured, supplied).is_err());
    }
    for field in 0..4 {
        let mut supplied = basis();
        match field {
            0 => supplied.qualification_reference = " ".into(),
            1 => supplied.qualification_scope = " ".into(),
            2 => supplied.uncertainty_source = "x".repeat(2_001),
            _ => supplied.reviewer = "x".repeat(201),
        }
        assert!(attach_decision_evidence(&mut record.clone(), &workspace, &image, &frame, &measured, supplied).is_err());
    }
    let duplicate = vec![measured[0].clone(), measured[0].clone()];
    assert!(attach_decision_evidence(&mut record.clone(), &workspace, &image, &frame, &duplicate, basis()).is_err());
}

#[test]
fn total_radius_u_is_used_as_supplied_without_diameter_propagation() {
    let (image, workspace, frame, mut record, measured) = committed("R2.5 ±0.2 mm");
    attach_decision_evidence(&mut record, &workspace, &image, &frame, &measured, basis()).unwrap();
    assert_eq!(record.outcome.conformity_status, ConformityStatus::Conforming);
    assert_eq!(record.decision_evidence.as_ref().unwrap().basis.expanded_uncertainty_mm, 0.02);
    assert!((crate::reference::measurement_mm(&measured[0]).unwrap() / 2.0 - record.value_mm.unwrap()).abs() < 1e-12);
    assert!(measured[0].expanded_uncertainty_mm.is_none());
}

#[test]
fn decision_requires_unique_stored_observation_and_same_plan_identity() {
    let (image, workspace, frame, record, measured) = reviewed();
    for change in 0..3 {
        let mut changed = workspace.clone();
        match change {
            0 => changed.records.clear(),
            1 => changed.records.push(changed.records[0].clone()),
            _ => changed.plan.as_mut().unwrap().source_name.push_str(" replaced"),
        }
        let outcome = assess_record(&record, &changed, &image, &frame, &measured);
        assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
        assert!(outcome.blocking_reasons.contains(&"decision_evidence_changed".into()));
        if change < 2 {
            assert!(attach_decision_evidence(&mut record.clone(), &changed, &image, &frame, &measured, basis()).is_err());
        }
    }
    let duplicate = vec![measured[0].clone(), measured[0].clone()];
    assert_eq!(assess_record(&record, &workspace, &image, &frame, &duplicate).conformity_status,
        ConformityStatus::Indeterminate);
}

#[test]
fn reviewed_decision_suspends_when_otherwise_valid_localization_context_changes() {
    let (image, mut workspace, frame, mut record, measured) = bound_fixture();
    workspace.records.push(record.clone());
    attach_decision_evidence(&mut record, &workspace, &image, &frame,
        &[measured.clone()], basis()).unwrap();
    assert_eq!(record.outcome.conformity_status, ConformityStatus::Conforming);
    for change in 0..3 {
        let mut changed = workspace.clone();
        match change {
            0 => changed.registration.as_mut().unwrap().transform.translation.x += 0.5,
            1 => changed.bindings[0].justification.push_str(" revised documentary context"),
            _ => changed.views[0].bounds.x_min -= 1.0,
        }
        let outcome = assess_record(&record, &changed, &image, &frame, &[measured.clone()]);
        // Translation/documentary changes do not alter source membership. A
        // mutated view envelope now also invalidates its source proof (DPM-38).
        if change == 2 {
            assert_ne!(outcome.geometry_state, GeometryState::Validated);
            assert!(outcome.blocking_reasons.contains(&"drawing_binding_changed".into()));
            assert!(outcome.blocking_reasons.contains(&"registration_missing_or_stale".into()));
        } else {
            assert_eq!(outcome.geometry_state, GeometryState::Validated);
        }
        assert_eq!(outcome.conformity_status, ConformityStatus::Indeterminate);
        assert!(outcome.blocking_reasons.contains(&"decision_evidence_changed".into()));
    }
}
