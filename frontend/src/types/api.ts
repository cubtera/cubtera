// Core types matching Rust backend structures

export interface DimensionData {
  name: string;
  type: string;
  meta?: Record<string, unknown>;
  config?: Record<string, unknown>;
  terraform?: Record<string, unknown>;
  test?: Record<string, unknown>;
}

export interface Dimension {
  dim_name: string;
  dim_type: string;
  key_path: string;
  dim_path: string;
  parent?: Dimension;
  data: Record<string, unknown>;
  data_sha: string;
  kids?: string[];
}

export interface UnitManifest {
  dimensions: string[];
  overwrite: boolean;
  opt_dims?: string[];
  allow_list?: string[];
  deny_list?: string[];
  affinity_tags?: string[];
  type: string; // unit_type in Rust
  spec?: {
    tf_version?: string;
    env_vars?: {
      required?: Record<string, string>;
      optional?: Record<string, string>;
    };
    files?: Record<string, unknown>;
  };
  runner?: Record<string, string>;
  state?: Record<string, string>;
}

export interface DeploymentLog {
  unit_name?: string;
  state_path?: string;
  dims?: Record<string, string>;
  job_host_name?: string;
  job_user_name?: string;
  job_number?: string;
  job_name?: string;
  tf_command?: string;
  exitcode?: number;
  unit_sha?: string;
  unit_blob_sha?: string;
  inventory_sha?: string;
  dims_blob_sha?: Record<string, string>;
  env_vars?: Record<string, string>;
  timestamp?: number;
  datetime?: string;
  extended_log?: Record<string, string>;
}

export interface CubteraConfig {
  workspace_path: string;
  inventory_path: string;
  units_path: string;
  modules_path: string;
  plugins_path: string;
  org: string;
  temp_folder_path: string;
  orgs: string[];
  dim_relations: string[];
  db?: string;
  dlog_db?: string;
  dlog_job_user_name_env?: string;
  dlog_job_number_env?: string;
  dlog_job_name_env?: string;
  clean_cache: boolean;
  always_copy_files: boolean;
}

// API Response types
export type ApiResponse<T> = T;

export type DimensionType = string;
export type DimensionName = string;

// Storage types
export type StorageType = 'FS' | 'DB';

// Deployment status
export type DeploymentStatus = 'pending' | 'running' | 'success' | 'failed';

// Runner types
export type RunnerType = 'tf' | 'bash' | 'helm'; 