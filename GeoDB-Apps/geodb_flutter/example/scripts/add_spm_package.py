#!/usr/bin/env python3
"""
Add GeodbKit SPM package to Xcode project programmatically.
This modifies the project.pbxproj file to add the local SPM package dependency.
"""

import os
import sys
import re
import uuid

def generate_uuid():
    """Generate a unique ID for Xcode objects."""
    return ''.join(str(uuid.uuid4()).upper().split('-'))[:24]

def add_spm_to_project(pbxproj_path, spm_package_path):
    """Add SPM package reference to Xcode project file."""

    if not os.path.exists(pbxproj_path):
        print(f"❌ Error: Project file not found: {pbxproj_path}")
        return False

    if not os.path.exists(spm_package_path):
        print(f"❌ Error: SPM package not found: {spm_package_path}")
        return False

    # Read project file
    with open(pbxproj_path, 'r') as f:
        content = f.read()

    # Check if already added
    if 'GeodbKit' in content or 'SPM-GeoDB-ffi' in content:
        print("ℹ️  GeodbKit package already referenced in project")
        return True

    print(f"📝 Adding GeodbKit SPM package to project...")

    # Generate UUIDs for new objects
    package_ref_id = generate_uuid()
    product_ref_id = generate_uuid()

    # 1. Add XCLocalSwiftPackageReference
    package_reference = f"""
/* Begin XCLocalSwiftPackageReference section */
		{package_ref_id} /* XCLocalSwiftPackageReference "SPM-GeoDB-ffi" */ = {{
			isa = XCLocalSwiftPackageReference;
			relativePath = "{spm_package_path}";
		}};
/* End XCLocalSwiftPackageReference section */
"""

    # Find where to insert package reference (before XCRemoteSwiftPackageReference or at the end)
    if '/* Begin XCRemoteSwiftPackageReference section */' in content:
        content = content.replace(
            '/* Begin XCRemoteSwiftPackageReference section */',
            package_reference + '\n/* Begin XCRemoteSwiftPackageReference section */'
        )
    else:
        # Add before the end of the file (before rootObject)
        content = re.sub(
            r'(rootObject = [A-Z0-9]+ /\* Project object \*/;)',
            package_reference + r'\n\1',
            content
        )

    # 2. Add XCSwiftPackageProductDependency
    product_dependency = f"""
		{product_ref_id} /* GeodbKit */ = {{
			isa = XCSwiftPackageProductDependency;
			package = {package_ref_id} /* XCLocalSwiftPackageReference "SPM-GeoDB-ffi" */;
			productName = GeodbKit;
		}};
"""

    # Find XCSwiftPackageProductDependency section or create it
    if '/* Begin XCSwiftPackageProductDependency section */' in content:
        # Add to existing section
        content = re.sub(
            r'(/\* Begin XCSwiftPackageProductDependency section \*/)',
            r'\1' + product_dependency,
            content
        )
    else:
        # Create new section
        product_section = f"""
/* Begin XCSwiftPackageProductDependency section */
{product_dependency}
/* End XCSwiftPackageProductDependency section */
"""
        content = re.sub(
            r'(rootObject = [A-Z0-9]+ /\* Project object \*/;)',
            product_section + r'\n\1',
            content
        )

    # 3. Add package dependency to Runner target
    # Find the Runner target's PBXNativeTarget section
    runner_target_match = re.search(
        r'([A-Z0-9]+) /\* Runner \*/ = \{[^}]*isa = PBXNativeTarget;[^}]*name = Runner;[^}]*packageProductDependencies = \((.*?)\);',
        content,
        re.DOTALL
    )

    if runner_target_match:
        target_id = runner_target_match.group(1)
        current_deps = runner_target_match.group(2).strip()

        if current_deps:
            new_deps = current_deps + f',\n\t\t\t\t{product_ref_id} /* GeodbKit */,'
        else:
            new_deps = f'\n\t\t\t\t{product_ref_id} /* GeodbKit */,\n\t\t\t'

        content = re.sub(
            rf'({target_id} /\* Runner \*/ = \{{[^}}]*packageProductDependencies = \()(.*?)(\);)',
            rf'\1{new_deps}\3',
            content,
            flags=re.DOTALL
        )
    else:
        print("⚠️  Warning: Could not find Runner target to add package dependency")
        print("   You may need to add the package manually in Xcode")

    # 4. Add package reference to project's package references
    # Find the PBXProject section
    project_match = re.search(
        r'([A-Z0-9]+) /\* Project object \*/ = \{[^}]*isa = PBXProject;',
        content
    )

    if project_match:
        # Look for packageReferences in the project
        if 'packageReferences = (' in content:
            content = re.sub(
                r'(packageReferences = \()',
                rf'\1\n\t\t\t\t{package_ref_id} /* XCLocalSwiftPackageReference "SPM-GeoDB-ffi" */,',
                content
            )
        else:
            # Add packageReferences field to project
            content = re.sub(
                r'(isa = PBXProject;)',
                rf'\1\n\t\t\t\tpackageReferences = (\n\t\t\t\t\t{package_ref_id} /* XCLocalSwiftPackageReference "SPM-GeoDB-ffi" */,\n\t\t\t\t);',
                content
            )

    # Write back
    with open(pbxproj_path, 'w') as f:
        f.write(content)

    print("✅ Successfully added GeodbKit SPM package to project")
    return True

def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    example_dir = os.path.dirname(script_dir)
    spm_package_path = "/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi"

    print("=" * 70)
    print("Adding GeodbKit SPM Package to Xcode Projects")
    print("=" * 70)
    print()

    # Get platform from command line or process both
    platforms = sys.argv[1:] if len(sys.argv) > 1 else ['ios', 'macos']

    success = True
    for platform in platforms:
        print(f"\n{'─' * 70}")
        print(f"Processing {platform.upper()} project...")
        print('─' * 70)

        pbxproj_path = os.path.join(
            example_dir,
            platform,
            'Runner.xcodeproj',
            'project.pbxproj'
        )

        if not os.path.exists(pbxproj_path):
            print(f"⚠️  Skipping {platform}: project.pbxproj not found")
            continue

        # Make backup
        backup_path = pbxproj_path + '.backup'
        import shutil
        shutil.copy2(pbxproj_path, backup_path)
        print(f"💾 Backup created: {backup_path}")

        if not add_spm_to_project(pbxproj_path, spm_package_path):
            success = False
            # Restore backup
            shutil.copy2(backup_path, pbxproj_path)
            print(f"⚠️  Restored from backup due to error")
        else:
            print(f"✅ {platform.upper()} project updated successfully")

    print()
    print("=" * 70)
    if success:
        print("✅ All projects updated successfully!")
        print()
        print("Next steps:")
        print("  1. Clean build folders:")
        print("     flutter clean")
        print("  2. Get dependencies:")
        print("     flutter pub get")
        print("  3. Build:")
        print("     flutter build ios --debug")
        print("     flutter build macos --debug")
    else:
        print("⚠️  Some projects failed to update")
        print("You may need to add the SPM package manually in Xcode:")
        print("  File → Add Package Dependencies... → Add Local...")
        print(f"  Select: {spm_package_path}")
    print("=" * 70)
    print()

if __name__ == '__main__':
    main()
