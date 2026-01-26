#!/usr/bin/env python3
"""
Perceptual hash deduplication for Cloudinary images.
Finds visually similar images and deletes duplicates.
"""

import os
import sys
import json
import requests
from io import BytesIO
from collections import defaultdict
from PIL import Image
import imagehash

# Cloudinary credentials
CLOUD_NAME = "ejf"
API_KEY = "772121974764543"
API_SECRET = "OaPOrn409H_wXnhS3eR6Y8B-4WY"

def fetch_all_images():
    """Fetch all images from Cloudinary."""
    print("Fetching all images from Cloudinary...")

    all_resources = []
    cursor = None
    page = 0

    while True:
        page += 1
        url = f"https://api.cloudinary.com/v1_1/{CLOUD_NAME}/resources/search"

        payload = {"max_results": 500}
        if cursor:
            payload["next_cursor"] = cursor

        response = requests.post(
            url,
            auth=(API_KEY, API_SECRET),
            json=payload
        )

        data = response.json()
        resources = data.get("resources", [])
        all_resources.extend(resources)

        print(f"  Page {page}: {len(resources)} images (total: {len(all_resources)})")

        cursor = data.get("next_cursor")
        if not cursor:
            break

    return all_resources

def compute_phash(url, size=128):
    """Download image and compute perceptual hash."""
    try:
        # Use Cloudinary transformation for smaller download
        thumb_url = url.replace("/upload/", f"/upload/c_fill,w_{size},h_{size}/")

        response = requests.get(thumb_url, timeout=10)
        response.raise_for_status()

        img = Image.open(BytesIO(response.content))
        return str(imagehash.phash(img, hash_size=16))
    except Exception as e:
        return None

def hamming_distance(hash1, hash2):
    """Calculate hamming distance between two hex hash strings."""
    if len(hash1) != len(hash2):
        return 999

    # Convert hex to binary and count differences
    bin1 = bin(int(hash1, 16))[2:].zfill(len(hash1) * 4)
    bin2 = bin(int(hash2, 16))[2:].zfill(len(hash2) * 4)

    return sum(c1 != c2 for c1, c2 in zip(bin1, bin2))

def group_similar(images_with_hashes, threshold=10):
    """Group images by similar perceptual hash."""
    groups = []
    used = set()

    items = [(img, h) for img, h in images_with_hashes if h is not None]

    for i, (img1, hash1) in enumerate(items):
        if img1["public_id"] in used:
            continue

        group = [img1]
        used.add(img1["public_id"])

        for j, (img2, hash2) in enumerate(items[i+1:], i+1):
            if img2["public_id"] in used:
                continue

            if hamming_distance(hash1, hash2) <= threshold:
                group.append(img2)
                used.add(img2["public_id"])

        if len(group) > 1:
            groups.append(group)

    return groups

def delete_images(public_ids, batch_size=100):
    """Delete images from Cloudinary."""
    deleted = 0

    for i in range(0, len(public_ids), batch_size):
        batch = public_ids[i:i+batch_size]

        url = f"https://api.cloudinary.com/v1_1/{CLOUD_NAME}/resources/image/upload"

        response = requests.delete(
            url,
            auth=(API_KEY, API_SECRET),
            data={"public_ids[]": batch}
        )

        result = response.json()
        batch_deleted = sum(1 for v in result.get("deleted", {}).values() if v == "deleted")
        deleted += batch_deleted

        print(f"  Deleted {deleted}/{len(public_ids)}...")

    return deleted

def main():
    # Fetch all images
    images = fetch_all_images()
    print(f"\nTotal images: {len(images)}")

    # Filter to scrapbook screenshots (main source of dupes)
    screenshots = [img for img in images if img.get("folder", "").startswith("scrapbook")]
    print(f"Scrapbook images to analyze: {len(screenshots)}")

    # Compute perceptual hashes
    print("\nComputing perceptual hashes (this takes a while)...")
    images_with_hashes = []

    for i, img in enumerate(screenshots):
        if i % 50 == 0:
            print(f"  Processing {i}/{len(screenshots)}...")

        phash = compute_phash(img["secure_url"])
        images_with_hashes.append((img, phash))

    # Group similar images
    print("\nGrouping similar images...")
    groups = group_similar(images_with_hashes, threshold=12)

    print(f"\nFound {len(groups)} groups of similar images")

    # Calculate duplicates to delete (keep oldest in each group)
    to_delete = []
    for group in groups:
        # Sort by created_at, keep oldest
        sorted_group = sorted(group, key=lambda x: x.get("created_at", ""))
        to_delete.extend([img["public_id"] for img in sorted_group[1:]])

    print(f"Images to delete: {len(to_delete)}")

    if not to_delete:
        print("Nothing to delete!")
        return

    # Save list before deleting
    with open("/tmp/phash_duplicates.json", "w") as f:
        json.dump(to_delete, f, indent=2)
    print(f"Saved duplicate list to /tmp/phash_duplicates.json")

    # Confirm and delete
    confirm = input("\nDelete these images? (yes/no): ")
    if confirm.lower() != "yes":
        print("Aborted.")
        return

    print("\nDeleting duplicates...")
    deleted = delete_images(to_delete)
    print(f"\nDone! Deleted {deleted} visually similar duplicates.")

if __name__ == "__main__":
    main()
