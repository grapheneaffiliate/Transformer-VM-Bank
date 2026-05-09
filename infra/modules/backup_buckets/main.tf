variable "hot_bucket"  { type = string }
variable "cold_bucket" { type = string }
variable "retention_d" { type = number, default = 90 }

resource "aws_s3_bucket" "hot" {
  bucket        = var.hot_bucket
  force_destroy = false   # never accidentally rm a backup bucket
}

resource "aws_s3_bucket_versioning" "hot" {
  bucket = aws_s3_bucket.hot.id
  versioning_configuration { status = "Enabled" }
}

resource "aws_s3_bucket_lifecycle_configuration" "hot" {
  bucket = aws_s3_bucket.hot.id
  rule {
    id     = "expire-old"
    status = "Enabled"
    expiration { days = var.retention_d }
  }
}

resource "aws_s3_bucket" "cold" {
  bucket        = var.cold_bucket
  force_destroy = false
}

resource "aws_s3_bucket_versioning" "cold" {
  bucket = aws_s3_bucket.cold.id
  versioning_configuration { status = "Enabled" }
}

# Cold tier retention is longer; backups uploaded with --storage-class GLACIER
# by tools/backup.sh. Lifecycle deletes after 1 year unless override.
resource "aws_s3_bucket_lifecycle_configuration" "cold" {
  bucket = aws_s3_bucket.cold.id
  rule {
    id     = "expire-old-cold"
    status = "Enabled"
    expiration { days = 365 }
  }
}

output "hot_bucket"  { value = aws_s3_bucket.hot.bucket }
output "cold_bucket" { value = aws_s3_bucket.cold.bucket }
output "hot_uri"     { value = "s3://${aws_s3_bucket.hot.bucket}/" }
output "cold_uri"    { value = "s3://${aws_s3_bucket.cold.bucket}/" }
