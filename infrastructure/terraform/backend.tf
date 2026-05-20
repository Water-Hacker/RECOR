# Remote state backend. Per-environment overrides supplied via
# `terraform init -backend-config="environments/dev.backend.tfvars"`.
# DynamoDB lock prevents concurrent applies.

terraform {
  backend "s3" {
    # Per-env: bucket, key, region, dynamodb_table.
    encrypt = true
  }
}
