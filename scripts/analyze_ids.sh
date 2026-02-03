#!/bin/bash

# File: /Users/htr/Documents/develeop/rust/geodb-rs/scripts/analyze_ids.sh

# Check if path argument is provided
if [ $# -eq 0 ]; then
    # Default to the data directory
    DATASET_PATH="$(dirname "$0")/../data/countries+states+cities.json.gz"
else
    DATASET_PATH="$1"
fi

# Check if the dataset exists
if [ ! -f "$DATASET_PATH" ]; then
    echo "Error: Dataset not found at: $DATASET_PATH"
    echo ""
    echo "Usage: $0 [path/to/dataset.json.gz]"
    echo "  If no path is provided, defaults to: ../data/countries+states+cities.json.gz"
    exit 1
fi

echo "Analyzing dataset: $DATASET_PATH"
echo "======================================="
echo ""

# Decompress and analyze
gzcat "$DATASET_PATH" | jq '
  {
    "max_country_id": [.[].id // 0] | max,
    "max_state_id": [.[].states[]?.id // 0] | max,
    "max_city_id": [.[].states[]?.cities[]?.id // 0] | max,
    "total_countries": length,
    "total_states": [.[].states | length] | add,
    "total_cities": [.[].states[]?.cities | length] | add
  }
'

echo ""
echo "Recommended types based on ranges:"
echo "-----------------------------------"
echo "  0 - 255:                    u8"
echo "  0 - 65,535:                 u16"
echo "  0 - 4,294,967,295:          u32"
echo "  0 - 9,223,372,036,854,775,807: u64"
echo ""
echo "Note: Use unsigned types (u8, u16, u32, u64) if IDs are always positive."
echo "      Use signed types (i8, i16, i32, i64) if negative values are possible."