"""Verify bounded triangular-tip hypotheses using actual synthetic PDF/MCP inputs."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing, number
from smoke_inspection_mcp import Mcp, fixtures

PREFIX = 'geometric_triangular_tip_attachment_hypothesis:'


def traces(candidate):
    return [json.loads(line.split(':', 1)[1]) for line in candidate['evidence'] if line.startswith(PREFIX)]


def run(folder, port):
    fixtures(folder)
    client = Mcp(folder, port)
    prior = client.call('get_measurement_context')
    assert prior['calibration']['id'] == 'synthetic-independent-reference', 'Refuse an unknown active context'
    client.call('save_project', {'path': str(folder / 'previous_synthetic_context.dperproj'), 'overwrite': False})
    initial = client.call('open_project', {'path': str(folder / 'fixture.dperproj')})
    calibration = initial['calibration']
    cases = ['solid', 'split', 'reverse_vertices', 'rotated', 'reflected', 'unfilled', 'backwards',
             'base_gap', 'tip_gap', 'wide_triangle', 'competing_tips', 'competing_supports',
             'branch', 'crossing', 'triangle_outside_scope', 'legacy_competition', 'independent_bad_component']
    positive = {'solid', 'split', 'reverse_vertices', 'rotated', 'reflected', 'independent_bad_component'}
    results = []
    for name in cases:
        drawing = Drawing()
        drawing.text(12, 20, f'SYNTHETIC TRIANGULAR TIP: {name}', 9)
        transform = lambda point: point
        if name == 'rotated':
            transform = lambda point: (360-point[1], point[0]-40)
        elif name == 'reflected':
            transform = lambda point: (400-point[0], point[1])
        target_center = transform((160, 160))
        drawing.circle(*target_center, 30)
        drawing.circle(*transform((300, 80)), 20)
        support = [(258, 166), (230, 166), (230, 160), (196, 160)]
        triangle = [(190, 160), (196, 158.5), (196, 161.5)]
        if name == 'reflected':
            # Keep the upright label separate from the reflected leader and circle.
            support = [(112, 230), (220, 230), (220, 160), (196, 160)]
        elif name == 'backwards':
            support = [(258, 166), (230, 166), (170, 160), (196, 160)]
        elif name == 'base_gap':
            # The base spans y=158.5..161.5: use a point genuinely outside it.
            support[-1] = (196, 163.25)
        elif name == 'tip_gap':
            triangle = [(194, 160), (200, 158.5), (200, 161.5)]
            support[-1] = (200, 160)
        elif name == 'wide_triangle':
            triangle = [(190, 160), (196, 156), (196, 164)]
        elif name == 'legacy_competition':
            triangle = [(189, 160), (192, 159.25), (192, 160.75)]
            support[-1] = (192, 160)
        if name == 'split':
            drawing.polyline([transform(point) for point in support[:2]])
            drawing.polyline([transform(point) for point in support[1:]])
        else:
            drawing.polyline([transform(point) for point in support])
        if name == 'competing_supports':
            drawing.polyline([(258, 165), (235, 165), (235, 160), (196, 160)])
        elif name == 'branch':
            drawing.polyline([(230, 160), (220, 145)])
        elif name == 'crossing':
            drawing.polyline([(210, 150), (210, 170)])
        elif name == 'independent_bad_component':
            for edge in [[(20, 40), (40, 40)], [(40, 40), (40, 60)], [(40, 60), (20, 40)]]:
                drawing.polyline(edge)
        if name == 'reverse_vertices':
            triangle.reverse()
        def tip(points, filled=True):
            points = [transform(point) for point in points]
            drawing.command(f'{number(points[0][0])} {number(points[0][1])} m '
                + ' '.join(f'{number(x)} {number(y)} l' for x, y in points[1:]) + (' h f' if filled else ' h S'))
        tip(triangle, name != 'unfilled')
        if name == 'competing_tips':
            tip([(191, 160), (196, 158.75), (196, 161.25)])
        start = transform(support[0])
        drawing.text(start[0]+2, start[1]+4, 'Ø4.000 ±0.100 mm')
        path = folder / f'{name}.pdf'
        path.write_bytes(drawing.pdf_bytes())
        source_hash = hashlib.sha256(path.read_bytes()).hexdigest()
        context = client.call('get_measurement_context')
        state = client.call('analyze_drawing', {'path': str(path), 'expected_revision': context['document_revision']})
        feature, = [item for item in state['inspection']['plan']['characteristics'] if item['kind'] == 'diameter']
        assert feature['nominal'] == 4 and not feature['confirmed'], (name, feature)
        catalog = client.call('analyze_drawing_geometry', {'page': 1, 'expected_revision': state['document_revision']})
        listing = client.call('get_drawing_geometries', {'page': 1, 'limit': 100})
        circles = [item for item in listing['items'] if item['kind'] == 'circle_candidate']
        assert len(circles) == 2, (name, circles)
        target = next(item for item in circles if abs(item['circle']['center']['x']-target_center[0]) < .1
                      and abs(item['circle']['center']['y']-target_center[1]) < .1)
        geometry_ids = [item['id'] for item in listing['items']
                        if name != 'triangle_outside_scope' or item['kind'] != 'closed_polygon']
        defined = client.call('define_drawing_view', {'expected_revision': catalog['document_revision'], 'page': 1,
            'label': f'Explicit synthetic scope {name}', 'feature_ids': [feature['id']], 'geometry_ids': geometry_ids})
        arguments = {'feature_id': feature['id'], 'view_id': defined['view']['id']}
        response = client.call('get_drawing_binding_candidates', arguments)
        choices = response['candidates']
        assert len(choices) == 2 and all('binding_requires_explicit_review' in item['suspension_reasons'] for item in choices)
        candidate = next(item for item in choices if item['geometry_id'] == target['id'])
        evidence = traces(candidate)
        if name in positive:
            assert len(evidence) == 1 and choices[0]['geometry_id'] == target['id'], (name, choices)
            trace, = evidence
            assert trace['unique_within_selected_view'] and trace['requires_review']
            assert len(trace['ordered_supports']) == (2 if name == 'split' else 1)
            assert len(trace['joints']) == (1 if name == 'split' else 0)
            assert trace['triangle']['source']['path_data'] and trace['triangle']['triangle_fingerprint']
            assert len(trace['triangle']['vertices']) == 3
            assert trace['base_gap_pt'] <= .5 and trace['circle_gap_pt'] <= 2 and trace['direction_cosine'] >= .98
            assert trace['triangle']['height_pt'] >= 3 and trace['triangle']['base_width_pt'] <= 6
            if name == 'solid':
                assert candidate['distance_pt'] > min(item['distance_pt'] for item in choices), 'No nearer distractor'
        elif name in ('competing_tips', 'legacy_competition'):
            assert len(evidence) == (2 if name == 'competing_tips' else 1), (name, choices)
            assert all(not trace['unique_within_selected_view'] for trace in evidence), (name, evidence)
        else:
            assert not any(traces(item) for item in choices), (name, choices)
        assert client.call('get_drawing_binding_candidates', arguments) == response
        state = client.call('get_inspection_state')
        assert not state['inspection']['bindings'] and not state['inspection']['records']
        chosen = client.call('select_drawing_binding', {'expected_revision': state['document_revision'], **arguments,
            'candidate_id': candidate['id'], 'justification': 'Synthetic geometric hypothesis only; explicit plan review remains required.'})
        assert not chosen['binding']['reviewed']
        assert chosen['binding']['candidate']['evidence'] == candidate['evidence']
        assert client.call('get_measurement_context')['calibration'] == calibration
        assert hashlib.sha256(path.read_bytes()).hexdigest() == source_hash
        results.append({'case': name, 'passed': True, 'source_sha256': source_hash, 'candidates': choices})
    summary = {'passed': True, 'cases': results, 'physical_qualification': 'not_performed',
               'scope': 'Bounded explicit triangular shape, not normative arrow or automatic binding recognition'}
    (folder / 'tips_summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({'passed': True, 'case_count': len(results)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folder', type=Path)
    parser.add_argument('--port', type=int, default=18766)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    destination = args.folder.resolve()
    if destination == root or not destination.is_relative_to(root) or destination.exists():
        parser.error('Use a new child folder of this checkout to preserve existing evidence.')
    if not 18765 <= args.port <= 65535:
        parser.error('Use an owned dedicated test port, never the live user server.')
    run(destination, args.port)
