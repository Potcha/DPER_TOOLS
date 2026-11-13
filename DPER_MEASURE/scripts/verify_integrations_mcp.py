"""Targeted DPM-4/11/15 integration recipe on an owned, empty MCP test host."""
import argparse
import json
import math
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from PIL import Image, ImageDraw
from smoke_inspection_mcp import Mcp
from verify_datum_support_mcp import fixture as datum_fixture
from verify_line_intersection_mcp import search


def run(folder, port):
    folder.mkdir(parents=True, exist_ok=False)
    client = Mcp(folder, port)
    checks = []
    tools = {tool['name'] for tool in client.rpc('tools/list', {})['tools']}
    assert {'select_drawing_datum_support', 'inspect_native_diagnostic', 'apply_observation_uncertainty'} <= tools

    image = Image.new('RGB', (256, 256), (30, 30, 30))
    draw = ImageDraw.Draw(image)
    draw.rectangle((51, 91, 190, 160), fill=(220, 220, 220))
    # Contrast outside the search supports exercises the default provisional quality screen.
    for y in range(205, 245, 5):
        for x in range(10, 245, 5):
            draw.rectangle((x, y, x + 4, y + 4), fill=(60 if (x + y) % 10 else 210,) * 3)
    image.save(folder / 'edge.png')
    calibration = {'id': 'independent-synthetic', 'device': 'Synthetic', 'resolution': [256, 256],
        'optical_setup': 'Synthetic plane', 'mm_per_pixel': .01, 'reference_mm': 1., 'reference_px': 100.,
        'origin': 'synthetic_fixture', 'validation_status': 'unverified'}
    project = folder / 'fixture.dperproj'
    project.write_text(json.dumps({'schema_version': 21, 'capture_id': 'integration-synthetic', 'image_file': 'edge.png',
        'calibration': calibration, 'calibrations': [calibration], 'measurements': []}), encoding='utf-8')
    context = client.call('open_project', {'path': str(project)})
    assert context['image_quality_assessment']['is_exploitable'], context
    request = {'capture_id': context['capture_id'], 'expected_revision': context['document_revision'],
        'feature_id': 'visible-edge', 'group': 'DPM-11 visible edges', 'commit': True,
        'request': {'kind': 'straight_edge_endpoints', 'search': {
            'edge': search((53.5, 92), (187.5, 92)),
            'start_boundary': search((49, 93.5), (49, 130)),
            'end_boundary': search((192, 93.5), (192, 130))}}}
    client.call('inspect_native_diagnostic', request, expect_error=True)
    assert client.call('get_measurement_context') == context
    preview = client.call('inspect_native_diagnostic', dict(request, commit=False))
    assert preview['record']['measurement_id'] is None
    assert not client.call('get_measurement_context')['measurements']
    recorded = client.call('inspect_native_diagnostic', dict(request, expected_revision=preview['document_revision']))
    record = recorded['record']
    assert abs(record['diagnostic']['result']['length_px'] - 140.) < .35
    assert math.isclose(record['value_mm'], 1.4, abs_tol=.0035)
    assert record['outcome']['geometry_state'] == 'validated', record['outcome']
    assert record['outcome']['conformity_status'] == 'indeterminate'
    measurements = client.call('get_measurement_context')['measurements']
    assert len(measurements) == 4
    checks += ['legacy schema 21 opened', 'direct commit refused without mutation', 'preview creates no drawing',
               'native segment and three source constructions recorded', 'observed 140 px / 1.4 mm; conformity indeterminate']

    matrix = [[float(i == j) for j in range(5)] for i in range(5)]
    point_u = {'standard_uncertainty_x_px': .2, 'standard_uncertainty_y_px': .2, 'source': 'Synthetic point model'}
    budget = {'capture_id': context['capture_id'], 'expected_revision': recorded['document_revision'],
        'measurement_id': record['measurement_id'], 'input': {'model': 'distance', 'points': [point_u, point_u],
        'scale': {'relative_standard_uncertainty': .01, 'source': 'Independent synthetic scale'},
        'propagation': {'coverage_factor': 2., 'correlation_matrix': matrix, 'correlation_justification': 'Synthetic independence'}}}
    calculated = client.call('apply_observation_uncertainty', budget)
    expected_u = 2. * math.hypot(.01 * math.sqrt(2.) * .2, record['value_mm'] * .01)
    assert math.isclose(calculated['calculation']['expanded_uncertainty'], expected_u, rel_tol=1e-10)
    assert calculated['attached_to_observation'] and not calculated['applied_to_measurement']
    assert not calculated['source_qualification_verified'] and not calculated['conformity_evaluated']
    state = client.call('get_inspection_state')
    assert state['observation_uncertainty_status'][0]['current'], state
    assert client.call('get_measurement_context')['measurements'] == measurements
    client.call('apply_observation_uncertainty', budget, expect_error=True)
    bad = dict(budget, expected_revision=state['document_revision'], input=dict(budget['input'], radius=4))
    client.call('apply_observation_uncertainty', bad, expect_error=True)
    assert client.call('get_inspection_state') == state
    checks += ['distance budget matches independent analytical oracle', 'budget replay current, native uncertainty unchanged',
               'stale budget and injected geometry refused without mutation']

    saved = folder / 'integrated.dperproj'
    client.call('save_project', {'path': str(saved), 'overwrite': False})
    data = json.loads(saved.read_text(encoding='utf-8-sig'))
    assert data['schema_version'] == 22
    preparation = client.call('prepare_report', {'folder': str(folder)})
    selected = [item['key'] for item in preparation['items'] if item['measurement_count'] > 0 and item['name'].startswith('integrated')]
    assert len(selected) == 1, preparation
    exported = client.call('export_report', {'capture_id': context['capture_id'], 'expected_revision': preparation['document_revision'],
        'folder': str(folder), 'selected_image_keys': selected, 'format': 'pdf', 'destination': str(folder / 'integrated.pdf'),
        'include_audit_page': False, 'persist_preferences': False, 'overwrite': False})
    manifest = json.loads(Path(exported['manifest_path']).read_text(encoding='utf-8-sig'))
    assert 'observation_uncertainty' in json.dumps(manifest) and 'diagnostic' in json.dumps(manifest)
    client.call('open_project', {'path': str(saved)})
    reopened = client.call('get_inspection_state')
    assert not reopened['observation_uncertainty_status'][0]['current']
    assert reopened['inspection']['records'][-1]['imported']
    checks += ['schema 22 and PDF/manifest preserve proofs', 'reopened budget and observation require reassessment']
    # Actual PDF extraction and saved documentary proposal; no remote review authority.
    pdf = folder / 'datum.pdf'
    datum_fixture(pdf)
    state = client.call('get_inspection_state')
    plan = client.call('analyze_drawing', {'path': str(pdf), 'expected_revision': state['document_revision']})
    catalog = client.call('analyze_drawing_geometry', {'page': 1, 'expected_revision': plan['document_revision']})
    feature = client.call('get_inspection_state')['inspection']['plan']['characteristics'][0]['id']
    items = client.call('get_drawing_geometries', {'page': 1, 'limit': 100})['items']
    support = next(item['id'] for item in items if item['kind'] == 'line')
    view = client.call('define_drawing_view', {'expected_revision': catalog['document_revision'], 'page': 1,
        'label': 'Documentary datum', 'feature_ids': [feature], 'geometry_ids': [support]})
    args = {'expected_revision': view['document_revision'], 'feature_id': feature, 'view_id': view['view']['id'],
        'label': 'A', 'support_id': support, 'justification': 'Synthetic documentary review only.'}
    proposal = client.call('select_drawing_datum_support', args)
    assert proposal['requires_native_confirmation'] and not proposal['association']['reviewed']
    before = client.call('get_inspection_state')
    client.call('select_drawing_datum_support', dict(args, expected_revision=before['document_revision'], reviewed=True), expect_error=True)
    assert client.call('get_inspection_state') == before
    assert not before['drawing_datum_association_status'][0]['documentary_link_resolved']
    client.call('save_project', {'path': str(folder / 'datum-review.dperproj'), 'overwrite': False})
    checks += ['PDF datum proposal persisted', 'MCP cannot forge native documentary review']

    summary = {'passed': True, 'checks': checks, 'length_px': record['diagnostic']['result']['length_px'],
        'value_mm': record['value_mm'], 'expanded_uncertainty_mm': calculated['calculation']['expanded_uncertainty'],
        'physical_qualification': False, 'port': port}
    (folder / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--test-port', type=int, required=True)
    args = parser.parse_args()
    if not 18765 <= args.test_port <= 65535:
        parser.error('Use a dedicated owned test port >= 18765.')
    run(args.folder.resolve(), args.test_port)
