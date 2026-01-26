#!/bin/bash
# Cloudinary duplicate cleanup script
# Deletes duplicate files (keeps oldest version of each)

set -e

API_KEY="772121974764543"
API_SECRET="OaPOrn409H_wXnhS3eR6Y8B-4WY"
CLOUD_NAME="ejf"

DUPLICATES_FILE="/tmp/duplicates_to_delete.json"

if [ ! -f "$DUPLICATES_FILE" ]; then
    echo "Error: Duplicates file not found. Run the analysis first."
    exit 1
fi

total=$(jq 'length' "$DUPLICATES_FILE")
echo "=== CLOUDINARY DUPLICATE CLEANUP ==="
echo "Total duplicates to delete: $total"
echo ""
echo "This will free up approximately 2.7 GB of storage."
echo ""
read -p "Are you sure you want to delete these files? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    echo "Aborted."
    exit 0
fi

echo ""
echo "Starting deletion..."

# Cloudinary's delete API accepts up to 100 public_ids at a time
batch_size=100
deleted=0
failed=0

# Process in batches
for ((i=0; i<total; i+=batch_size)); do
    # Extract batch of public_ids
    batch=$(jq ".[$i:$((i+batch_size))]" "$DUPLICATES_FILE")
    batch_count=$(echo "$batch" | jq 'length')

    # Build the delete request
    # Format: public_ids[]=id1&public_ids[]=id2&...
    params=""
    for id in $(echo "$batch" | jq -r '.[]'); do
        params="${params}public_ids[]=$(echo "$id" | jq -sRr @uri)&"
    done

    # Make delete request
    response=$(curl -s -X DELETE \
        "https://api.cloudinary.com/v1_1/${CLOUD_NAME}/resources/image/upload" \
        -u "${API_KEY}:${API_SECRET}" \
        -d "${params%&}")

    # Check result
    deleted_count=$(echo "$response" | jq '.deleted | to_entries | map(select(.value == "deleted")) | length' 2>/dev/null || echo 0)
    not_found=$(echo "$response" | jq '.deleted | to_entries | map(select(.value == "not_found")) | length' 2>/dev/null || echo 0)

    deleted=$((deleted + deleted_count))
    failed=$((failed + not_found))

    echo "Progress: $((i + batch_count))/$total (deleted: $deleted, not found: $failed)"

    # Rate limiting - be nice to the API
    sleep 0.5
done

echo ""
echo "=== CLEANUP COMPLETE ==="
echo "Successfully deleted: $deleted"
echo "Not found (already deleted?): $failed"
echo ""
echo "Run 'npm run tauri dev' to see the cleaned up media library!"
