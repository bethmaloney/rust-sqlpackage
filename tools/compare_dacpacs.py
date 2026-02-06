#!/usr/bin/env python3
"""Compare two dacpac files (or extracted directories) and report differences.

Usage:
    python3 tools/compare_dacpacs.py <rust_dacpac> <dotnet_dacpac>
    python3 tools/compare_dacpacs.py /path/to/rust.dacpac /path/to/dotnet.dacpac
    python3 tools/compare_dacpacs.py /path/to/extracted_rust/ /path/to/extracted_dotnet/
"""

import argparse
import difflib
import os
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile

NS = "http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02"
NS_BRACKET = f"{{{NS}}}"


def strip_ns(tag):
    """Remove the MS namespace from an XML tag."""
    if tag.startswith(NS_BRACKET):
        return tag[len(NS_BRACKET):]
    return tag


def extract_dacpac(path, tmpdir):
    """Extract a dacpac ZIP to a temp directory. Returns the extraction path."""
    extract_to = os.path.join(tmpdir, os.path.basename(path))
    with zipfile.ZipFile(path) as z:
        z.extractall(extract_to)
    return extract_to


def list_files(directory):
    """List all files in a directory (non-recursive, top-level only)."""
    if not os.path.isdir(directory):
        return set()
    return set(f for f in os.listdir(directory) if os.path.isfile(os.path.join(directory, f)))


def compare_text_files(path_a, path_b, label):
    """Compare two text files with normalized whitespace. Returns list of diff strings."""
    if not os.path.exists(path_a) and not os.path.exists(path_b):
        return None, []  # Neither exists, skip
    if not os.path.exists(path_a):
        return "missing_in_rust", [f"  File missing in rust dacpac"]
    if not os.path.exists(path_b):
        return "missing_in_dotnet", [f"  File missing in dotnet dacpac"]

    with open(path_a) as f:
        lines_a = [line.rstrip() for line in f.readlines()]
    with open(path_b) as f:
        lines_b = [line.rstrip() for line in f.readlines()]

    if lines_a == lines_b:
        return "ok", []

    diff = list(difflib.unified_diff(lines_b, lines_a, fromfile="dotnet", tofile="rust",
                                     lineterm="", n=3))
    return "different", diff


def xml_to_canonical(elem):
    """Convert an XML element to a canonical string for comparison."""
    parts = [strip_ns(elem.tag)]
    for k in sorted(elem.attrib.keys()):
        parts.append(f"{k}={elem.attrib[k]}")
    text = (elem.text or "").strip()
    if text:
        parts.append(f"text={text}")
    return "|".join(parts)


def compare_simple_xml(path_a, path_b, label):
    """Compare two simple XML files by canonical string of all elements (order-independent)."""
    if not os.path.exists(path_a) and not os.path.exists(path_b):
        return None, []
    if not os.path.exists(path_a):
        return "missing_in_rust", [f"  File missing in rust dacpac"]
    if not os.path.exists(path_b):
        return "missing_in_dotnet", [f"  File missing in dotnet dacpac"]

    tree_a = ET.parse(path_a)
    tree_b = ET.parse(path_b)

    def flatten(elem, path=""):
        """Flatten XML tree to sorted list of (path, canonical_string) tuples."""
        current = path + "/" + strip_ns(elem.tag)
        result = [(current, xml_to_canonical(elem))]
        for child in sorted(elem, key=lambda c: xml_to_canonical(c)):
            result.extend(flatten(child, current))
        return result

    flat_a = flatten(tree_a.getroot())
    flat_b = flatten(tree_b.getroot())

    if flat_a == flat_b:
        return "ok", []

    # Show differences
    lines_a = [f"{p}: {c}" for p, c in flat_a]
    lines_b = [f"{p}: {c}" for p, c in flat_b]
    diff = list(difflib.unified_diff(lines_b, lines_a, fromfile="dotnet", tofile="rust",
                                     lineterm="", n=2))
    return "different", diff


# --- model.xml comparison ---

def get_ref_name(rel_elem, rel_name):
    """Get the Name attribute of the first References in a named Relationship."""
    rel = rel_elem.find(f"{NS_BRACKET}Relationship[@Name='{rel_name}']")
    if rel is None:
        return None
    entry = rel.find(f"{NS_BRACKET}Entry")
    if entry is None:
        return None
    ref = entry.find(f"{NS_BRACKET}References")
    if ref is not None:
        return ref.get("Name")
    return None


def element_key(elem):
    """Generate a unique key for a model Element.

    Named elements: (Type, Name)
    Unnamed elements: (Type, composite from DefiningTable + ForColumn or DefiningTable)
    Singletons (no name, no relationships): (Type,)
    """
    elem_type = elem.get("Type")
    name = elem.get("Name")
    if name is not None:
        return (elem_type, name)

    # Try DefiningTable + ForColumn (e.g. SqlDefaultConstraint)
    defining_table = get_ref_name(elem, "DefiningTable")
    for_column = get_ref_name(elem, "ForColumn")
    defining_column = get_ref_name(elem, "DefiningColumn")

    if defining_table and for_column:
        return (elem_type, f"DefiningTable={defining_table},ForColumn={for_column}")
    if defining_table and defining_column:
        return (elem_type, f"DefiningTable={defining_table},DefiningColumn={defining_column}")
    if defining_table:
        return (elem_type, f"DefiningTable={defining_table}")

    # Singleton type (e.g. SqlDatabaseOptions)
    return (elem_type,)


def element_display_key(key):
    """Format an element key for display."""
    if len(key) == 1:
        return key[0]
    return f"{key[0]} {key[1]}"


def get_properties(elem):
    """Extract properties as dict: name -> value (text content or Value attribute)."""
    props = {}
    for prop in elem.findall(f"{NS_BRACKET}Property"):
        name = prop.get("Name")
        value = prop.get("Value")
        if value is None:
            # Check for child Value element with CDATA/text
            val_elem = prop.find(f"{NS_BRACKET}Value")
            if val_elem is not None:
                value = (val_elem.text or "").strip()
        props[name] = value
    return props


def get_relationships(elem):
    """Extract relationships as dict: name -> list of reference keys."""
    rels = {}
    for rel in elem.findall(f"{NS_BRACKET}Relationship"):
        rel_name = rel.get("Name")
        entries = []
        for entry in rel.findall(f"{NS_BRACKET}Entry"):
            ref = entry.find(f"{NS_BRACKET}References")
            if ref is not None:
                ref_key = ref.get("Name", "")
                ext = ref.get("ExternalSource")
                if ext:
                    ref_key = f"{ref_key}@{ext}"
                entries.append(("ref", ref_key))
            else:
                # Inline element
                inline = entry.find(f"{NS_BRACKET}Element")
                if inline is not None:
                    entries.append(("inline", inline_element_fingerprint(inline)))
        rels[rel_name] = entries
    return rels


def inline_element_fingerprint(elem):
    """Create a fingerprint of an inline element for comparison (order-independent)."""
    type_part = elem.get("Type", "")
    # Properties (sorted by Name for order-independence)
    prop_parts = []
    for prop in elem.findall(f"{NS_BRACKET}Property"):
        name = prop.get("Name")
        value = prop.get("Value")
        if value is None:
            val_elem = prop.find(f"{NS_BRACKET}Value")
            if val_elem is not None:
                value = (val_elem.text or "").strip()
        prop_parts.append(f"P:{name}={value}")
    prop_parts.sort()
    # Nested relationships (sorted by rel_name then ref for order-independence)
    rel_parts = []
    for rel in elem.findall(f"{NS_BRACKET}Relationship"):
        rel_name = rel.get("Name")
        for entry in rel.findall(f"{NS_BRACKET}Entry"):
            ref = entry.find(f"{NS_BRACKET}References")
            if ref is not None:
                ref_name = ref.get("Name", "")
                ext = ref.get("ExternalSource")
                if ext:
                    ref_name = f"{ref_name}@{ext}"
                rel_parts.append(f"R:{rel_name}={ref_name}")
            else:
                inner = entry.find(f"{NS_BRACKET}Element")
                if inner is not None:
                    rel_parts.append(f"R:{rel_name}=({inline_element_fingerprint(inner)})")
    rel_parts.sort()
    # Annotations (sorted by type for order-independence)
    ann_parts = []
    for ann in elem.findall(f"{NS_BRACKET}AttachedAnnotation"):
        ann_type = ann.get("Type", "")
        ann_props = get_properties(ann)
        # Skip Disambiguator
        ann_props.pop("Disambiguator", None)
        ann_parts.append(f"A:{ann_type}={ann_props}")
    ann_parts.sort()
    return "|".join([type_part] + prop_parts + rel_parts + ann_parts)


def get_annotations(elem):
    """Extract annotations as sorted list of (type, properties) tuples.

    Uses a list instead of a dict to handle multiple annotations of the same type.
    Sorted for order-independent comparison.
    """
    anns = []
    for ann in elem.findall(f"{NS_BRACKET}AttachedAnnotation"):
        ann_type = ann.get("Type", "")
        props = get_properties(ann)
        # Disambiguator values are sequential IDs that may differ between builds
        props.pop("Disambiguator", None)
        anns.append((ann_type, sorted(props.items())))
    anns.sort()
    return anns


def diff_element(elem_a, elem_b):
    """Compare two elements and return list of difference descriptions."""
    diffs = []

    # Compare properties
    props_a = get_properties(elem_a)
    props_b = get_properties(elem_b)
    all_prop_names = sorted(set(props_a.keys()) | set(props_b.keys()))
    for name in all_prop_names:
        val_a = props_a.get(name)
        val_b = props_b.get(name)
        if val_a != val_b:
            if name not in props_b:
                diffs.append(f"    Property \"{name}\": missing in dotnet, rust=\"{val_a}\"")
            elif name not in props_a:
                diffs.append(f"    Property \"{name}\": dotnet=\"{val_b}\", missing in rust")
            else:
                diffs.append(f"    Property \"{name}\": dotnet=\"{val_b}\", rust=\"{val_a}\"")

    # Compare relationships
    rels_a = get_relationships(elem_a)
    rels_b = get_relationships(elem_b)
    all_rel_names = sorted(set(rels_a.keys()) | set(rels_b.keys()))
    for name in all_rel_names:
        entries_a = rels_a.get(name, [])
        entries_b = rels_b.get(name, [])
        if entries_a != entries_b:
            if name not in rels_b:
                diffs.append(f"    Relationship \"{name}\": missing in dotnet, rust has {len(entries_a)} entries")
            elif name not in rels_a:
                diffs.append(f"    Relationship \"{name}\": dotnet has {len(entries_b)} entries, missing in rust")
            else:
                # Compare entry by entry
                set_a = set(str(e) for e in entries_a)
                set_b = set(str(e) for e in entries_b)
                only_rust = set_a - set_b
                only_dotnet = set_b - set_a
                if only_rust or only_dotnet:
                    diffs.append(f"    Relationship \"{name}\": {len(only_dotnet)} only in dotnet, {len(only_rust)} only in rust")

    # Compare annotations (sorted lists of (type, props) tuples)
    anns_a = get_annotations(elem_a)
    anns_b = get_annotations(elem_b)
    if anns_a != anns_b:
        # Show which annotation types differ
        set_a = set(str(a) for a in anns_a)
        set_b = set(str(a) for a in anns_b)
        only_rust = set_a - set_b
        only_dotnet = set_b - set_a
        if only_rust or only_dotnet:
            types_affected = set()
            for a in anns_a + anns_b:
                types_affected.add(a[0])
            for ann_type in sorted(types_affected):
                rust_of_type = [a for a in anns_a if a[0] == ann_type]
                dotnet_of_type = [a for a in anns_b if a[0] == ann_type]
                if rust_of_type != dotnet_of_type:
                    count_info = ""
                    if len(rust_of_type) != len(dotnet_of_type):
                        count_info = f" (rust={len(rust_of_type)}, dotnet={len(dotnet_of_type)})"
                    diffs.append(f"    Annotation \"{ann_type}\": differs{count_info}")

    return diffs


def compare_header(header_a, header_b):
    """Compare Header/CustomData sections. Returns (status, diff_lines)."""
    def index_custom_data(header):
        """Index CustomData by (Category, Type) -> list of Metadata dicts."""
        result = {}
        if header is None:
            return result
        for cd in header.findall(f"{NS_BRACKET}CustomData"):
            key = (cd.get("Category", ""), cd.get("Type", ""))
            metas = {}
            for m in cd.findall(f"{NS_BRACKET}Metadata"):
                metas[m.get("Name", "")] = m.get("Value", "")
            result[key] = metas
        return result

    cd_a = index_custom_data(header_a)
    cd_b = index_custom_data(header_b)
    all_keys = sorted(set(cd_a.keys()) | set(cd_b.keys()))

    diffs = []
    for key in all_keys:
        label = f"CustomData({key[0]}, {key[1]})" if key[1] else f"CustomData({key[0]})"
        if key not in cd_b:
            diffs.append(f"  {label}: missing in dotnet")
        elif key not in cd_a:
            diffs.append(f"  {label}: missing in rust")
        elif cd_a[key] != cd_b[key]:
            diffs.append(f"  {label}:")
            all_meta = sorted(set(cd_a[key].keys()) | set(cd_b[key].keys()))
            for mk in all_meta:
                va = cd_a[key].get(mk)
                vb = cd_b[key].get(mk)
                if va != vb:
                    diffs.append(f"    {mk}: dotnet=\"{vb}\", rust=\"{va}\"")

    if not diffs:
        return "ok", []
    return "different", diffs


def compare_model_xml(path_a, path_b):
    """Compare two model.xml files semantically. Returns structured results."""
    results = {}

    tree_a = ET.parse(path_a)
    tree_b = ET.parse(path_b)
    root_a = tree_a.getroot()
    root_b = tree_b.getroot()

    # Compare headers
    header_a = root_a.find(f"{NS_BRACKET}Header")
    header_b = root_b.find(f"{NS_BRACKET}Header")
    results["header"] = compare_header(header_a, header_b)

    # Compare model elements
    model_a = root_a.find(f"{NS_BRACKET}Model")
    model_b = root_b.find(f"{NS_BRACKET}Model")

    # Index elements by key
    def index_elements(model):
        index = {}
        duplicates = []
        for elem in model.findall(f"{NS_BRACKET}Element"):
            key = element_key(elem)
            if key in index:
                duplicates.append(key)
            index[key] = elem
        return index, duplicates

    elems_a, dupes_a = index_elements(model_a)
    elems_b, dupes_b = index_elements(model_b)

    if dupes_a:
        print(f"WARNING: {len(dupes_a)} duplicate keys in rust model.xml", file=sys.stderr)
        for d in dupes_a[:5]:
            print(f"  {element_display_key(d)}", file=sys.stderr)
    if dupes_b:
        print(f"WARNING: {len(dupes_b)} duplicate keys in dotnet model.xml", file=sys.stderr)
        for d in dupes_b[:5]:
            print(f"  {element_display_key(d)}", file=sys.stderr)

    keys_a = set(elems_a.keys())
    keys_b = set(elems_b.keys())

    missing_in_rust = sorted(keys_b - keys_a, key=lambda k: element_display_key(k))
    extra_in_rust = sorted(keys_a - keys_b, key=lambda k: element_display_key(k))
    common = sorted(keys_a & keys_b, key=lambda k: element_display_key(k))

    # Diff common elements
    differences = []
    for key in common:
        diffs = diff_element(elems_a[key], elems_b[key])
        if diffs:
            differences.append((key, diffs))

    results["elements"] = {
        "missing_in_rust": missing_in_rust,
        "extra_in_rust": extra_in_rust,
        "differences": differences,
        "total_rust": len(elems_a),
        "total_dotnet": len(elems_b),
    }

    return results


def print_report(file_results, model_results):
    """Print the comparison report."""
    print("=== Dacpac Comparison Report ===")
    print()

    for label, (status, lines) in file_results.items():
        if status is None:
            continue
        print(f"--- {label} ---")
        if status == "ok":
            print("OK (identical)")
        else:
            for line in lines[:50]:
                print(line)
            if len(lines) > 50:
                print(f"  ... ({len(lines) - 50} more lines)")
        print()

    if model_results is None:
        return

    # Header
    h_status, h_lines = model_results["header"]
    print("--- model.xml: Header ---")
    if h_status == "ok":
        print("OK (identical)")
    else:
        for line in h_lines:
            print(line)
    print()

    # Elements
    elems = model_results["elements"]
    print("--- model.xml: Elements ---")
    print(f"Total elements: rust={elems['total_rust']}, dotnet={elems['total_dotnet']}")
    print()

    missing = elems["missing_in_rust"]
    print(f"Missing in rust ({len(missing)}):")
    if missing:
        for key in missing:
            print(f"  {element_display_key(key)}")
    else:
        print("  (none)")
    print()

    extra = elems["extra_in_rust"]
    print(f"Extra in rust ({len(extra)}):")
    if extra:
        for key in extra:
            print(f"  {element_display_key(key)}")
    else:
        print("  (none)")
    print()

    diffs = elems["differences"]
    print(f"Differences ({len(diffs)}):")
    if diffs:
        for key, diff_lines in diffs:
            print(f"  {element_display_key(key)}:")
            for line in diff_lines:
                print(line)
    else:
        print("  (none)")
    print()

    print(f"Summary: {len(missing)} missing, {len(extra)} extra, {len(diffs)} different")


def main():
    parser = argparse.ArgumentParser(description="Compare two dacpac files")
    parser.add_argument("rust_dacpac", help="Path to rust-generated dacpac or extracted directory")
    parser.add_argument("dotnet_dacpac", help="Path to dotnet-generated dacpac or extracted directory")
    args = parser.parse_args()

    with tempfile.TemporaryDirectory() as tmpdir:
        # Resolve paths - extract if dacpac, use directly if directory
        if os.path.isdir(args.rust_dacpac):
            dir_a = args.rust_dacpac
        else:
            dir_a = extract_dacpac(args.rust_dacpac, tmpdir)

        if os.path.isdir(args.dotnet_dacpac):
            dir_b = args.dotnet_dacpac
        else:
            dir_b = extract_dacpac(args.dotnet_dacpac, tmpdir)

        # Compare non-model files
        file_results = {}

        # Origin.xml - skip (timestamps/GUIDs always differ)
        file_results["Origin.xml"] = ("ok", ["(skipped - contains timestamps/GUIDs)"])

        # Simple XML files
        for fname in ["DacMetadata.xml", "[Content_Types].xml"]:
            status, lines = compare_simple_xml(
                os.path.join(dir_a, fname),
                os.path.join(dir_b, fname),
                fname,
            )
            if status is not None:
                file_results[fname] = (status, lines)

        # Text files
        for fname in ["predeploy.sql", "postdeploy.sql"]:
            status, lines = compare_text_files(
                os.path.join(dir_a, fname),
                os.path.join(dir_b, fname),
                fname,
            )
            if status is not None:
                file_results[fname] = (status, lines)

        # Check for unexpected files not covered by the comparisons above
        known_files = {"Origin.xml", "DacMetadata.xml", "[Content_Types].xml",
                       "predeploy.sql", "postdeploy.sql", "model.xml"}
        files_a = list_files(dir_a)
        files_b = list_files(dir_b)
        unknown_only_rust = sorted(files_a - files_b - known_files)
        unknown_only_dotnet = sorted(files_b - files_a - known_files)
        unknown_both = sorted((files_a & files_b) - known_files)
        if unknown_only_rust:
            file_results["(unexpected files)"] = (
                "different",
                [f"  Only in rust: {', '.join(unknown_only_rust)}"],
            )
        if unknown_only_dotnet:
            label = "(unexpected files)" if "(unexpected files)" not in file_results else "(unexpected files in dotnet)"
            file_results[label] = (
                "different",
                [f"  Only in dotnet: {', '.join(unknown_only_dotnet)}"],
            )
        if unknown_both:
            for fname in unknown_both:
                status, lines = compare_text_files(
                    os.path.join(dir_a, fname),
                    os.path.join(dir_b, fname),
                    fname,
                )
                if status is not None and status != "ok":
                    file_results[fname] = (status, lines)

        # model.xml
        model_a = os.path.join(dir_a, "model.xml")
        model_b = os.path.join(dir_b, "model.xml")
        model_results = None
        if os.path.exists(model_a) and os.path.exists(model_b):
            model_results = compare_model_xml(model_a, model_b)
        elif not os.path.exists(model_a):
            file_results["model.xml"] = ("missing_in_rust", ["  File missing in rust dacpac"])
        elif not os.path.exists(model_b):
            file_results["model.xml"] = ("missing_in_dotnet", ["  File missing in dotnet dacpac"])

        print_report(file_results, model_results)

        # Exit code: 0 if identical, 1 if differences found
        has_diffs = False
        for label, (status, _) in file_results.items():
            if label == "Origin.xml":
                continue
            if status not in ("ok", None):
                has_diffs = True
        if model_results:
            if model_results["header"][0] != "ok":
                has_diffs = True
            elems = model_results["elements"]
            if elems["missing_in_rust"] or elems["extra_in_rust"] or elems["differences"]:
                has_diffs = True

        sys.exit(1 if has_diffs else 0)


if __name__ == "__main__":
    main()
