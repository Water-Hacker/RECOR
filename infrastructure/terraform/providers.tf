# RÉCOR Terraform — provider versions.
#
# Version pins are tight per D19 (reproducible builds). Bump only via
# an ADR that documents the upgrade path and re-runs the
# `terraform plan` against every environment.

terraform {
  required_version = ">= 1.10.0, < 2.0.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.80"
    }
    google = {
      source  = "hashicorp/google"
      version = "~> 6.15"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.34"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.17"
    }
    vault = {
      source  = "hashicorp/vault"
      version = "~> 4.5"
    }
  }
}

provider "aws" {
  region = var.aws_region
  default_tags {
    tags = {
      Project     = "recor"
      Environment = var.environment
      ManagedBy   = "terraform"
    }
  }
}

provider "kubernetes" {
  host                   = var.kubernetes_host
  cluster_ca_certificate = var.kubernetes_ca_pem
  token                  = var.kubernetes_token
}

provider "helm" {
  kubernetes {
    host                   = var.kubernetes_host
    cluster_ca_certificate = var.kubernetes_ca_pem
    token                  = var.kubernetes_token
  }
}
