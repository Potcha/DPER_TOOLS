"""Exercise documentary unit conflicts through real PDF extraction and owned MCP.

Synthetic inputs only. This checks interpretation, not physical qualification.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from generate_vector_drawing_fixtures import Drawing
from smoke_inspection_mcp import Mcp, fixtures


def pages_pdf(pages):
    """Small independent multi-page PDF wrapper around the synthetic drawing streams."""
    kids = ' '.join(f'{4 + 2*i} 0 R' for i in range(len(pages)))
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>',
               f'<< /Type /Pages /Kids [{kids}] /Count {len(pages)} >>'.encode(),
               b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>']
    for index, drawing in enumerate(pages):
        content = b'\n'.join(drawing.commands + [b'Q']) + b'\n'
        objects.extend([
            (f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 320] '
             f'/Resources << /Font << /F1 3 0 R >> >> /Contents {5+2*index} 0 R >>').encode(),
            b'<< /Length ' + str(len(content)).encode() + b' >>\nstream\n' + content + b'endstream'])
    result = bytearray(b'%PDF-1.4\n%\xe2\xe3\xcf\xd3\n')
    offsets = []
    for index, obj in enumerate(objects, 1):
        offsets.append(len(result))
        result.extend(f'{index} 0 obj\n'.encode() + obj + b'\nendobj\n')
    xref = len(result)
    result.extend(f'xref\n0 {len(objects)+1}\n0000000000 65535 f \n'.encode())
    for offset in offsets:
        result.extend(f'{offset:010d} 00000 n \n'.encode())
    result.extend(f'trailer\n<< /Size {len(objects)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    return bytes(result)


def run(folder, port):
    fixtures(folder)
    client = Mcp(folder, port)
    prior = client.call('get_measurement_context')
    assert prior['calibration']['id'] == 'synthetic-independent-reference', 'Refuse an unknown active context'
    # Preserve the previous synthetic acceptance context before opening this corpus.
    client.call('save_project', {'path': str(folder / 'previous_synthetic_context.dperproj'), 'overwrite': False})
    initial = client.call('open_project', {'path': str(folder / 'fixture.dperproj')})
    calibration = initial['calibration']
    # name, declarations per page, dimension text, documentary state, effective unit
    cases = [
        ('english_mm', [['All dimensions in mm']], 'Ø4.000 ±0.100', 'unique', 'millimeter'),
        ('french_mm', [['Toutes les cotes en mm']], 'Ø4.000 ±0.100', 'unique', 'millimeter'),
        ('inch', [['Units: inches']], 'Ø4.000 ±0.100', 'unique', 'inch'),
        ('same_unit', [['Units: mm', 'Units: millimeters']], 'Ø4.000 ±0.100', 'unique', 'millimeter'),
        ('same_line', [['Units: mm / inches']], 'Ø4.000 ±0.100', 'conflict', None),
        ('reversed', [['Units: inches / mm']], 'Ø4.000 ±0.100', 'conflict', None),
        ('two_lines', [['Units: mm', 'Units: inches']], 'Ø4.000 ±0.100', 'conflict', None),
        ('two_pages', [['Units: mm'], ['Units: inches']], 'Ø4.000 ±0.100', 'conflict', None),
        ('no_declaration', [[]], 'Ø4.000 ±0.100', 'absent', None),
        ('inline_conflict', [['Units: mm / inches']], 'Ø4.000 ±0.100 mm', 'conflict', 'millimeter'),
        ('mixed_tolerance', [['Units: mm']], 'Ø4.000 mm ±0.100 in', 'unique', 'millimeter'),
    ]
    results = []
    for name, declarations, dimension, status, unit in cases:
        drawings = []
        for page_index, lines in enumerate(declarations, 1):
            drawing = Drawing()
            drawing.text(15, 20, f'SYNTHETIC UNIT CONTEXT - PAGE {page_index}', 9)
            for index, line in enumerate(lines):
                drawing.text(20, 45 + index*20, line)
            drawing.circle(75, 170, 25)
            drawing.text(125, 170, dimension)
            drawings.append(drawing)
        path = folder / f'{name}.pdf'
        path.write_bytes(pages_pdf(drawings))
        source_hash = hashlib.sha256(path.read_bytes()).hexdigest()
        context = client.call('get_measurement_context')
        state = client.call('analyze_drawing', {'path': str(path), 'expected_revision': context['document_revision']})
        plan = state['inspection']['plan']
        documentary = plan['unit_context']
        assert documentary['status'] == status, (name, documentary)
        assert len(documentary['declarations']) == sum(map(len, declarations)), (name, documentary)
        for declaration in documentary['declarations']:
            assert declaration['page'] in range(1, len(drawings)+1)
            assert declaration['pdf_bbox'] and declaration['word_ids'], declaration
            assert declaration['exact_text'] in declarations[declaration['page']-1], declaration
        features = [item for item in plan['characteristics'] if item['kind'] == 'diameter']
        assert len(features) == len(drawings), (name, features)
        for feature in features:
            assert feature['nominal'] == 4 and feature['unit'] == unit and not feature['confirmed'], (name, feature)
            assert feature['unit_context']['document'] == documentary, feature
            expected_origin = 'inline' if name in ('inline_conflict', 'mixed_tolerance') else (
                'document_declaration' if unit else 'unspecified')
            assert feature['unit_context']['origin'] == expected_origin, feature
            if name == 'mixed_tolerance':
                assert feature['tolerance'] is None and feature['measurability'] == 'needs_confirmation', feature
            if status == 'conflict':
                assert any('unit_declarations_conflict' in reason for reason in feature['reasons'])
        if name == 'same_line':
            state = client.call('propose_drawing_interpretation', {
                'expected_revision': state['document_revision'], 'feature_id': features[0]['id'],
                'kind': 'diameter', 'unit': 'millimeter',
                'source_note': 'Synthetic unresolved declarations: propose mm for explicit operator review, not a measured scale.'})
            proposed = state['inspection']['plan']['characteristics'][0]
            assert not proposed['confirmed'] and proposed['measurability'] == 'needs_confirmation'
            assert proposed['unit_context']['origin'] == 'interpretation_proposal'
            assert proposed['unit_context']['document'] == documentary
            assert any('unit_declarations_conflict' in reason for reason in proposed['reasons'])
            saved = folder / 'conflict_roundtrip.dperproj'
            client.call('save_project', {'path': str(saved), 'overwrite': False})
            client.call('open_project', {'path': str(saved)})
            reloaded = client.call('get_inspection_state')['inspection']['plan']
            assert reloaded['unit_context'] == documentary
            assert reloaded['characteristics'][0]['unit_context'] == proposed['unit_context']
            assert not reloaded['characteristics'][0]['confirmed']
        assert not state['inspection']['records'] and not state['inspection']['bindings']
        current = client.call('get_measurement_context')
        assert current['calibration'] == calibration, 'Drawing changed independent calibration'
        assert hashlib.sha256(path.read_bytes()).hexdigest() == source_hash
        results.append({'case': name, 'passed': True, 'source_sha256': source_hash,
                        'page_count': len(drawings), 'unit_context': documentary, 'features': features})
    summary = {'passed': True, 'cases': results, 'physical_qualification': 'not_performed'}
    (folder / 'units_summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
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
