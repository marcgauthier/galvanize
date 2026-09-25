-- Overwatch Corrosion schema. Generated from backend/pkgs/db/gorm_models.go.
-- Only PRIMARY KEY constraints and non-unique performance indexes are used.

CREATE TABLE IF NOT EXISTS "apikey" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "key_hash" TEXT,
  "description" TEXT,
  "is_active" numeric,
  "last_used" datetime,
  "expires_at" datetime,
  "created_by" varchar(36)
);

CREATE TABLE IF NOT EXISTS "apikey_group" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "api_key_id" varchar(36),
  "group_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "assistant_conversations" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "user_id" varchar(36),
  "title" varchar(255),
  "provider" varchar(32),
  "updated_at" INTEGER
);

CREATE TABLE IF NOT EXISTS "assistant_messages" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "conversation_id" varchar(36),
  "role" varchar(16),
  "content" TEXT,
  "tool_name" varchar(128),
  "tool_arguments" TEXT,
  "tool_call_id" varchar(128),
  "tool_calls_json" TEXT,
  "created_at_unix" INTEGER
);

CREATE TABLE IF NOT EXISTS "assistant_tool_calls" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "conversation_id" varchar(36),
  "user_id" varchar(36),
  "tool_call_id" varchar(128),
  "tool_name" varchar(128),
  "arguments" TEXT,
  "risk" varchar(16),
  "summary" TEXT,
  "status" varchar(16),
  "expires_at" INTEGER,
  "resolved_at" INTEGER,
  "result" TEXT
);

CREATE TABLE IF NOT EXISTS "br" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "smc_priority" INTEGER,
  "dbrm_priority" INTEGER,
  "client_priority" INTEGER,
  "submit_date" datetime,
  "approval_date" datetime,
  "implementation_date" datetime,
  "estimate_completion_date" datetime,
  "completed_date" datetime,
  "status" TEXT,
  "description" TEXT,
  "site_id" varchar(36),
  "brbits_number" TEXT,
  "brd" TEXT,
  "justification" TEXT,
  "br_type_id" varchar(36),
  "br_status_id" varchar(36),
  "br_phase_id" varchar(36),
  "l1_id" varchar(36),
  "supportive_smc_id" varchar(36),
  "dnd_top10" numeric,
  "dnd_top50" numeric,
  "dnd_top100" numeric
);

CREATE TABLE IF NOT EXISTS "br_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "br_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "br_documents" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "br_id" varchar(36),
  "document_number" TEXT,
  "document_type" TEXT
);

CREATE TABLE IF NOT EXISTS "br_finance" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "br_id" varchar(36),
  "fy" TEXT,
  "funding_group" TEXT,
  "funding_contact" varchar(36),
  "amount" REAL,
  "financial_info" TEXT
);

CREATE TABLE IF NOT EXISTS "br_phase" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "br_project" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "br_id" varchar(36),
  "project_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "br_sd" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "br_id" varchar(36),
  "sd" TEXT
);

CREATE TABLE IF NOT EXISTS "br_status" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "br_type" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "building_telecom_room" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "building_id" varchar(36),
  "name" TEXT,
  "room_number" TEXT,
  "floor_number" TEXT,
  "room_type" TEXT,
  "description" TEXT,
  "square_footage" INTEGER,
  "rack_count" INTEGER,
  "rack_units_total" INTEGER,
  "rack_units_available" INTEGER,
  "power_circuits_count" INTEGER,
  "power_available_watts" INTEGER,
  "power_used_watts" INTEGER,
  "cooling_type" TEXT,
  "cooling_capacity_btu" INTEGER,
  "environmental_monitoring" numeric,
  "temperature_sensor" numeric,
  "humidity_sensor" numeric,
  "water_detection" numeric,
  "smoke_detection" numeric,
  "fire_suppression_type" TEXT,
  "access_control_type" TEXT,
  "keycard_required" numeric,
  "combination_code" TEXT,
  "access_list" TEXT,
  "has_raised_floor" numeric,
  "has_cable_management" numeric,
  "patch_panel_count" INTEGER,
  "fiber_termination_count" INTEGER,
  "copper_termination_count" INTEGER,
  "upstream_connection" TEXT,
  "backbone_cabling_type" TEXT,
  "horizontal_cabling_type" TEXT,
  "network_equipment" TEXT,
  "active_network_ports" INTEGER,
  "available_network_ports" INTEGER,
  "certification_date" datetime,
  "last_inspection_date" datetime,
  "next_inspection_date" datetime,
  "emergency_shutoff_location" TEXT,
  "contact_id" varchar(36),
  "status" TEXT,
  "notes" TEXT
);

CREATE TABLE IF NOT EXISTS "buildings" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "name" TEXT,
  "description" TEXT,
  "ao_house_num" TEXT,
  "street_name" TEXT,
  "building_number" TEXT,
  "city" TEXT,
  "province" TEXT,
  "postal_code" TEXT,
  "country" TEXT,
  "hosted_unit" TEXT,
  "number_of_users" INTEGER,
  "number_of_workstations" INTEGER,
  "max_occupancy" INTEGER,
  "number_of_floors" INTEGER,
  "total_square_footage" INTEGER,
  "networks_available" TEXT,
  "is_mdf" numeric,
  "has_datacenter" numeric,
  "has_server_room" numeric,
  "has_telecom_rooms" numeric,
  "power_capacity_kw" REAL,
  "has_generator" numeric,
  "generator_capacity_kw" REAL,
  "has_ups" numeric,
  "ups_capacity_kw" REAL,
  "ups_runtime_minutes" INTEGER,
  "fiber_entry_point" TEXT,
  "fiber_service_providers" TEXT,
  "structured_cabling_type" TEXT,
  "cable_plant_certification_date" datetime,
  "security_classification" TEXT,
  "emergency_contact_id" varchar(36),
  "building_manager_contact_id" varchar(36),
  "it_contact_id" varchar(36),
  "facility_hours" TEXT,
  "after_hours_access" TEXT,
  "parking_available" numeric,
  "loading_dock_access" numeric,
  "notes" TEXT
);

CREATE TABLE IF NOT EXISTS "cbas" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "apm_id" varchar(100),
  "departmental_application_id" varchar(100),
  "important_notes" TEXT,
  "name" varchar(255),
  "french_name" varchar(255),
  "acronym" varchar(100),
  "french_acronym" varchar(100),
  "network" varchar(100),
  "ecs" varchar(255),
  "ecsl1" varchar(255),
  "l1" varchar(255),
  "lifecycle" varchar(100),
  "department_owning_application" varchar(255),
  "application_supported_by" varchar(255),
  "hosting_technology" varchar(255),
  "infrastructure_supported_by" varchar(255),
  "primary_data_center" TEXT,
  "secondary_data_center" TEXT,
  "description_of_impact_of_unavailability" TEXT,
  "partial_outages_threshold_justification" TEXT,
  "used_by_ogd" varchar(100),
  "used_by_public_or_ngo" varchar(100),
  "ssc_required_for_it_continuity_plan" varchar(100),
  "monday_to_friday_dnd_application_support" varchar(100),
  "saturday_and_sunday_application_support" varchar(100),
  "dnd_service_desk_support_monday_to_friday" varchar(100),
  "dnd_service_desk_support_saturday_and_sunday" varchar(100),
  "expected_availability_monday_to_friday" varchar(100),
  "expected_availability_saturday_and_sunday" varchar(100)
);

CREATE TABLE IF NOT EXISTS "contacts" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "rank" TEXT,
  "organization" TEXT,
  "department" varchar(36),
  "location" TEXT,
  "note" TEXT,
  "status" TEXT,
  "manager" TEXT,
  "team" TEXT,
  "work_schedule" TEXT,
  "time_zone" TEXT,
  "l1_id" varchar(36),
  "street" TEXT,
  "city" TEXT,
  "province" TEXT,
  "postalcode" TEXT,
  "country" TEXT,
  "phones" TEXT,
  "emails" TEXT,
  "titles" TEXT
);

CREATE TABLE IF NOT EXISTS "departments" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "device_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "device_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "devices" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "type" INTEGER,
  "vendor" TEXT,
  "serial" TEXT,
  "model" TEXT,
  "asset_tag" TEXT,
  "asset_tag_ssc" TEXT,
  "rack" TEXT,
  "rack_u_position" INTEGER,
  "network_id" varchar(36),
  "service_id" varchar(36),
  "location" TEXT,
  "site_id" varchar(36),
  "building_id" varchar(36),
  "telecom_room_id" varchar(36),
  "room" TEXT,
  "dns_name" TEXT,
  "end_of_life_date" INTEGER,
  "support_queue_unclass" TEXT,
  "support_queue_classified" TEXT,
  "ntp_device_class" varchar(32),
  "ntp_time_source" varchar(32),
  "ntp_source_reference" varchar(255),
  "ntp_stratum" INTEGER,
  "ntp_reference_clock" numeric,
  "ntp_upstream_server" numeric,
  "ntp_downstream_server" numeric
);

CREATE TABLE IF NOT EXISTS "distribution_lists" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" varchar(255),
  "email_address" varchar(255),
  "group_name" varchar(255),
  "folder_name" TEXT,
  "webhook_url" TEXT,
  "type" varchar(50),
  "first_language" varchar(8),
  "template" varchar(255),
  "send_ima" numeric,
  "send_unplanned" numeric,
  "send_cir" numeric,
  "send_planned_start" numeric,
  "send_planned_end" numeric,
  "send_planned_submitted" numeric,
  "send_planned_approved" numeric,
  "send_planned_cancelled" numeric,
  "send_planned_rescheduled" numeric,
  "send_planned_pending" numeric,
  "send_planned_cfsr" numeric,
  "send_planned_software_update" numeric,
  "send_planned_other" numeric,
  "smc_ids" TEXT,
  "created_by" varchar(100)
);

CREATE TABLE IF NOT EXISTS "document_br" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "document_id" varchar(36),
  "br_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "document_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "document_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "document_project" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "document_id" varchar(36),
  "project_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "document_site" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "document_id" varchar(36),
  "site_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "documents" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "description" TEXT,
  "type" TEXT,
  "site_id" varchar(36),
  "contact_id" varchar(36),
  "br_id" varchar(36),
  "classification" TEXT,
  "created_by" varchar(36),
  "extracted_text" TEXT,
  "path" TEXT,
  "storage_type" TEXT,
  "file_size" INTEGER,
  "mime_type" TEXT,
  "original_filename" TEXT,
  "content_sha256" TEXT,
  "is_encrypted" numeric,
  "is_compressed" numeric
);

CREATE TABLE IF NOT EXISTS "geocache" (
  "address" TEXT,
  "city" TEXT,
  "province" TEXT,
  "country" TEXT,
  "lat" REAL,
  "lon" REAL,
  "created_at" INTEGER,
  "updated_at" INTEGER,
  PRIMARY KEY ("address", "city", "province", "country")
);

CREATE TABLE IF NOT EXISTS "gorm_chat_messages" (
  "id" INTEGER PRIMARY KEY,
  "msg_id" varchar(64),
  "time" INTEGER,
  "user_id" varchar(36),
  "username" varchar(100),
  "room" varchar(64),
  "message" TEXT
);

CREATE TABLE IF NOT EXISTS "gorm_chat_rooms" (
  "id" varchar(64) PRIMARY KEY,
  "name" varchar(100),
  "description" TEXT,
  "created_by" varchar(100),
  "created_at" INTEGER,
  "is_default" numeric
);

CREATE TABLE IF NOT EXISTS "group_permissions" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "group_id" TEXT,
  "table_name" TEXT,
  "permission" TEXT
);

CREATE TABLE IF NOT EXISTS "groups" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "description" TEXT
);

CREATE TABLE IF NOT EXISTS "import_checkpoints" (
  "source" TEXT PRIMARY KEY,
  "last_successful_poll" INTEGER,
  "updated_at" INTEGER
);

CREATE TABLE IF NOT EXISTS "interfaces" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "device_id" varchar(36),
  "name" TEXT,
  "type" INTEGER,
  "speed_mbps" INTEGER,
  "ipv4" TEXT,
  "ipv6" TEXT,
  "macd" TEXT,
  "subnet_mask" TEXT,
  "default_gateway" TEXT,
  "vlan" TEXT,
  "cable_type" INTEGER,
  "cable_max_speed_mbps" INTEGER,
  "to_interface_id" varchar(36),
  "connected_to_site_id" TEXT,
  "connected_to_building_id" TEXT,
  "connected_to_device_id" TEXT
);

CREATE TABLE IF NOT EXISTS "issue_updates" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "issue_id" varchar(36),
  "from_user" TEXT,
  "message" TEXT,
  "status" TEXT,
  "created_by" varchar(36)
);

CREATE TABLE IF NOT EXISTS "issues" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "title" TEXT,
  "description" TEXT,
  "status" TEXT,
  "priority" INTEGER,
  "severity" TEXT,
  "created_by" varchar(36),
  "assigned_to_l1" varchar(36),
  "site_id" varchar(36),
  "assigned_to_contact" varchar(36),
  "reported_date" datetime,
  "start_date" datetime,
  "end_date" datetime,
  "expected_resolution_date" datetime,
  "actual_resolution_date" datetime,
  "estimated_hours" INTEGER,
  "actual_hours" INTEGER,
  "category" TEXT,
  "type" TEXT,
  "impact" TEXT,
  "root_cause" TEXT,
  "resolution_notes" TEXT
);

CREATE TABLE IF NOT EXISTS "l1" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" varchar(100),
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT,
  "long_name" TEXT,
  "french_long_name" TEXT
);

CREATE TABLE IF NOT EXISTS "lias" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "name" TEXT,
  "source_record_id" varchar(100),
  "department_name" varchar(200),
  "description" TEXT,
  "justification" TEXT,
  "l1_id" varchar(36),
  "byod" numeric,
  "vendor" TEXT,
  "model" TEXT,
  "service_provider" TEXT,
  "circuit_id" TEXT,
  "secondary_circuit_id" varchar(100),
  "account_number" TEXT,
  "contract_number" varchar(100),
  "service_type" TEXT,
  "supplier_category" varchar(100),
  "status" TEXT,
  "connection_type" TEXT,
  "service_speed" TEXT,
  "service_attributes" TEXT,
  "ip_assignment" varchar(50),
  "ip_requirement" varchar(50),
  "usage_cap" varchar(100),
  "bandwidth_up_mbps" REAL,
  "bandwidth_down_mbps" REAL,
  "static_ip_block" TEXT,
  "gateway_ip" TEXT,
  "dns_primary" TEXT,
  "dns_secondary" TEXT,
  "vlan_id" INTEGER,
  "network_segment" TEXT,
  "contract_start_date" datetime,
  "contract_end_date" datetime,
  "service_start_date" datetime,
  "monthly_cost" REAL,
  "recurring_charge" REAL,
  "billing_account_number" varchar(100),
  "billing_period" varchar(6),
  "organization_code" varchar(50),
  "funding_type" varchar(100),
  "cost_center" varchar(50),
  "financial_code1" varchar(255),
  "financial_code2" varchar(255),
  "financial_code3" varchar(255),
  "sla_uptime_percent" REAL,
  "support_phone" TEXT,
  "support_email" TEXT,
  "contact_id" varchar(36),
  "is_primary" numeric,
  "is_backup" numeric,
  "failover_priority" INTEGER,
  "classification" TEXT,
  "notes" TEXT,
  "last_tested" datetime,
  "monitoring_enabled" numeric,
  "service_address" TEXT,
  "street_address" varchar(255),
  "floor" varchar(100),
  "room" varchar(100),
  "postal_code" varchar(20),
  "city" varchar(100),
  "locality" varchar(100),
  "province_code" varchar(2),
  "source_last_update_date" datetime,
  "source_created_date" datetime,
  "source_modified_date" datetime
);

CREATE TABLE IF NOT EXISTS "lias_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "lias_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "migration_metadata" (
  "key" TEXT PRIMARY KEY,
  "value" TEXT,
  "updated_at" INTEGER
);

CREATE TABLE IF NOT EXISTS "networks" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "long_name" varchar(200),
  "french_long_name" varchar(200),
  "security_level" varchar(20),
  "technical_authority" varchar(200),
  "french_technical_authority" varchar(200),
  "operational_authority" varchar(200),
  "french_operational_authority" varchar(200),
  "security_authority" varchar(200),
  "french_security_authority" varchar(200),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "news" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "title" TEXT,
  "message" TEXT,
  "label" TEXT,
  "show_start_time" INTEGER,
  "show_end_time" INTEGER,
  "is_active" INTEGER,
  "priority" INTEGER,
  "is_national" INTEGER,
  "smc_ids" TEXT,
  "created_by" varchar(36)
);

CREATE TABLE IF NOT EXISTS "oia" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" TEXT,
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "planned_authority" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" varchar(255),
  "french_name" varchar(255),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "planned_authority_user" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "authority_id" varchar(36),
  "user_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "planned_comments" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "authority_id" varchar(36),
  "comment" TEXT,
  "created_by" varchar(100)
);

CREATE TABLE IF NOT EXISTS "planned_votes" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "authority_id" varchar(36),
  "vote" varchar(20),
  "voted_at" INTEGER,
  "user_id" varchar(36),
  "comment" TEXT
);

CREATE TABLE IF NOT EXISTS "plannedevent_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "plannedevent_location" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "location_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "plannedevent_manual_occurrence" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "occurrence_time" INTEGER
);

CREATE TABLE IF NOT EXISTS "plannedevent_network" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "network_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "plannedevent_service" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "planned_event_id" varchar(36),
  "service_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "plannedevents" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "status" INTEGER,
  "createdbyuser" TEXT,
  "recurrentparentid" TEXT,
  "recurrentparenttime" INTEGER,
  "runningstatus" INTEGER,
  "starttime" INTEGER,
  "endtime" INTEGER,
  "type" TEXT,
  "title" TEXT,
  "title_french" TEXT,
  "ownership" TEXT,
  "national" TEXT,
  "major" numeric,
  "duration" TEXT,
  "scope" TEXT,
  "access" TEXT,
  "impact" TEXT,
  "ccts" TEXT,
  "userimpact" TEXT,
  "urgency" TEXT,
  "vip" TEXT,
  "contact" TEXT,
  "reviewed" TEXT,
  "legacyid" TEXT,
  "recurrence_type" TEXT,
  "recurrence_interval" TEXT,
  "recurrence_weekday" TEXT,
  "recurrence_week_of_month" TEXT,
  "recurrence_day_of_month" INTEGER,
  "recurrence_monthly_mode" TEXT,
  "recurrence_yearly_mode" TEXT,
  "recurrence_month_of_year" INTEGER,
  "recurrence_end_type" TEXT,
  "recurrence_end_date" INTEGER,
  "recurrence_count" INTEGER,
  "recurrence_start_time" TEXT,
  "recurrence_window_minutes" INTEGER,
  "next_event" INTEGER,
  "classifed" numeric,
  "details" TEXT,
  "link" TEXT,
  "rfc" TEXT,
  "assignedto" varchar(36),
  "master_ticket" TEXT,
  "tickets" TEXT,
  "all_services_affected" numeric,
  "all_networks_affected" numeric,
  "express" numeric,
  "express_note" TEXT,
  "express_creator_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "pls" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "name" TEXT,
  "application" TEXT,
  "telecom" TEXT,
  "vendor" TEXT,
  "contact_id" varchar(36),
  "customer_id" TEXT,
  "circuit_info" TEXT,
  "nap_id" TEXT,
  "nap_start_date" datetime,
  "full_duplex" numeric,
  "speed_mbps" INTEGER,
  "cost" REAL,
  "who_funding" TEXT,
  "description" TEXT,
  "site_a_site_id" varchar(36),
  "site_a_contact_id" varchar(36),
  "site_a_room" TEXT,
  "site_a_rack" TEXT,
  "site_a_contact_info" TEXT,
  "site_b_site_id" varchar(36),
  "site_b_contact_id" varchar(36),
  "site_b_room" TEXT,
  "site_b_rack" TEXT,
  "site_b_contact_info" TEXT
);

CREATE TABLE IF NOT EXISTS "pls_finance" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "pls_id" varchar(36),
  "fy" TEXT,
  "funding_group" TEXT,
  "funding_contact" varchar(36),
  "amount" REAL,
  "financial_info" TEXT
);

CREATE TABLE IF NOT EXISTS "project_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "project_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "projects" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "l1_id" varchar(36),
  "smc_id" varchar(36),
  "type" TEXT,
  "sponsor_group" TEXT,
  "sponsor_contact" varchar(36),
  "project_number" TEXT,
  "budget" REAL,
  "start_date" datetime,
  "end_date" datetime,
  "status" TEXT,
  "description" TEXT,
  "department" TEXT,
  "sponsor" TEXT
);

CREATE TABLE IF NOT EXISTS "queries" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "username" varchar(100),
  "query" TEXT,
  "time" INTEGER
);

CREATE TABLE IF NOT EXISTS "scn" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "name" TEXT,
  "application" TEXT,
  "telecom" TEXT,
  "vendor" TEXT,
  "contact_id" varchar(36),
  "customer_id" TEXT,
  "circuit_info" TEXT,
  "nap_id" TEXT,
  "nap_start_date" datetime,
  "full_duplex" numeric,
  "speed_mbps" INTEGER,
  "cost" REAL,
  "who_funding" TEXT,
  "description" TEXT,
  "site_a_site_id" varchar(36),
  "site_a_contact_id" varchar(36),
  "site_a_room" TEXT,
  "site_a_rack" TEXT,
  "site_a_contact_info" TEXT,
  "site_b_site_id" varchar(36),
  "site_b_contact_id" varchar(36),
  "site_b_room" TEXT,
  "site_b_rack" TEXT,
  "site_b_contact_info" TEXT
);

CREATE TABLE IF NOT EXISTS "scn_finance" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "scn_id" varchar(36),
  "fy" TEXT,
  "funding_group" TEXT,
  "funding_contact" varchar(36),
  "amount" REAL,
  "financial_info" TEXT
);

CREATE TABLE IF NOT EXISTS "sdp_transport" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "sdp_id" TEXT,
  "transport_id" TEXT,
  "ssc_cost" REAL,
  "dnd_cost" REAL,
  "dnd_payee" TEXT,
  "cost_until_fy" TEXT
);

CREATE TABLE IF NOT EXISTS "sdp_vrf" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "sdp_id" TEXT,
  "vrf_id" TEXT,
  "number_of_workstations" INTEGER,
  "bandwidth_mbps" REAL,
  "notes" TEXT
);

CREATE TABLE IF NOT EXISTS "servicelines" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "description" TEXT,
  "queue_unclas" TEXT,
  "queue_class" TEXT,
  "link_to_duty_line" TEXT,
  "business_hours" TEXT,
  "after_hours" TEXT,
  "department_id" varchar(36),
  "l1_id" varchar(36),
  "manager_contact_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "services" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "sessions" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "session_id" varchar(64),
  "username" varchar(100),
  "login_time" datetime,
  "expires_at" datetime
);

CREATE TABLE IF NOT EXISTS "site_access_type" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "site_br" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "br_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "site_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "site_project" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "project_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "site_sdp" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "connection_configuration" TEXT,
  "slt_sa" TEXT,
  "slt_mtrs" TEXT,
  "type_of_access" TEXT,
  "sdp_id" TEXT,
  "nap_id" TEXT,
  "scid_nap" TEXT,
  "scid_ctr" TEXT,
  "contract" TEXT,
  "provider" TEXT,
  "multicast" numeric,
  "sdp_address" TEXT,
  "total_bw" REAL,
  "lat" REAL,
  "lon" REAL,
  "transport_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "sites" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "alternate_name" TEXT,
  "url" varchar(2048),
  "description" TEXT,
  "city" TEXT,
  "province" TEXT,
  "country" TEXT,
  "lat" REAL,
  "lon" REAL,
  "status" TEXT,
  "markercolor" TEXT,
  "markerflash" numeric,
  "flashuntiltime" INTEGER,
  "deployed" numeric,
  "cbas" numeric,
  "smc_id" varchar(36),
  "l1_id" varchar(36),
  "sda_id" TEXT,
  "class" TEXT,
  "time_zone" TEXT,
  "support_24h" numeric,
  "ssc_region" TEXT,
  "cable_plant_last_update" INTEGER,
  "cable_plant_status" TEXT,
  "number_of_users_at_site" INTEGER,
  "hours_of_support" TEXT,
  "acronym" varchar(50),
  "french_name" varchar(100),
  "floor_suite_room" varchar(200),
  "requirement_name" TEXT,
  "designated_services" TEXT,
  "operational_impact" TEXT,
  "ecs_l1_details" TEXT,
  "known_departments" TEXT,
  "site_end_users_text" TEXT,
  "has_backup_generator" numeric,
  "shared_with_federal_departments" numeric,
  "public_service_point" numeric,
  "sda_migration_status" varchar(32),
  "point_of_contact" TEXT,
  "normal_business_hours_et" TEXT,
  "public_access_hours_et" TEXT,
  "after_hours_duty_tech" TEXT,
  "tenant_type" TEXT,
  "ssc_region_id" TEXT,
  "site_access_type_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "smc" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "l1_id" varchar(36),
  "name" TEXT,
  "long_name" TEXT,
  "manager" TEXT,
  "business_hours" TEXT,
  "description" TEXT,
  "after_hours" TEXT,
  "assyst_dwan" TEXT,
  "assyst_csni" TEXT,
  "phone" TEXT,
  "email" TEXT
);

CREATE TABLE IF NOT EXISTS "smc_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "smc_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "ssc_region" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "french_name" varchar(100),
  "description" TEXT,
  "french_description" TEXT
);

CREATE TABLE IF NOT EXISTS "stats_daily" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "day_timestamp" datetime,
  "endpoint" TEXT,
  "method" TEXT,
  "collection" TEXT,
  "operation" TEXT,
  "status_code" INTEGER,
  "request_count" INTEGER,
  "duration_sum" INTEGER,
  "duration_min" INTEGER,
  "duration_max" INTEGER,
  "duration_p50" INTEGER,
  "duration_p95" INTEGER,
  "duration_p99" INTEGER,
  "cache_hit_count" INTEGER,
  "cache_miss_count" INTEGER,
  "db_query_count_sum" INTEGER,
  "db_query_time_sum" INTEGER,
  "request_size_sum" INTEGER,
  "response_size_sum" INTEGER,
  "error_count" INTEGER,
  "unique_users" INTEGER
);

CREATE TABLE IF NOT EXISTS "stats_events" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "timestamp" datetime,
  "endpoint" TEXT,
  "method" TEXT,
  "collection" TEXT,
  "operation" TEXT,
  "duration_ms" INTEGER,
  "status_code" INTEGER,
  "request_size" INTEGER,
  "response_size" INTEGER,
  "cache_hit" numeric,
  "db_query_count" INTEGER,
  "db_query_time_ms" INTEGER,
  "user_id" TEXT,
  "username" TEXT,
  "error_message" TEXT,
  "user_agent" TEXT,
  "ip_address" TEXT
);

CREATE TABLE IF NOT EXISTS "stats_hourly" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "hour_timestamp" datetime,
  "endpoint" TEXT,
  "method" TEXT,
  "collection" TEXT,
  "operation" TEXT,
  "status_code" INTEGER,
  "request_count" INTEGER,
  "duration_sum" INTEGER,
  "duration_min" INTEGER,
  "duration_max" INTEGER,
  "duration_p50" INTEGER,
  "duration_p95" INTEGER,
  "duration_p99" INTEGER,
  "cache_hit_count" INTEGER,
  "cache_miss_count" INTEGER,
  "db_query_count_sum" INTEGER,
  "db_query_time_sum" INTEGER,
  "request_size_sum" INTEGER,
  "response_size_sum" INTEGER,
  "error_count" INTEGER,
  "unique_users" INTEGER
);

CREATE TABLE IF NOT EXISTS "stats_slow_queries" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "timestamp" datetime,
  "endpoint" TEXT,
  "collection" TEXT,
  "operation" TEXT,
  "duration_ms" INTEGER,
  "query_details" TEXT,
  "user_id" TEXT,
  "stack_trace" TEXT
);

CREATE TABLE IF NOT EXISTS "stats_user_activity" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "hour_timestamp" datetime,
  "user_id" TEXT,
  "username" TEXT,
  "request_count" INTEGER,
  "error_count" INTEGER,
  "avg_duration_ms" INTEGER,
  "top_collections" TEXT
);

CREATE TABLE IF NOT EXISTS "task_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "task_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "task_project" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "task_id" varchar(36),
  "project_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "task_site" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "task_id" varchar(36),
  "site_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "task_updates" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "task_id" varchar(36),
  "from_user" TEXT,
  "message" TEXT,
  "status" TEXT,
  "created_by" varchar(36)
);

CREATE TABLE IF NOT EXISTS "tasks" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "title" TEXT,
  "description" TEXT,
  "status" TEXT,
  "priority" INTEGER,
  "created_by" varchar(36),
  "assigned_to_l1" varchar(36),
  "site_id" varchar(36),
  "assigned_to_contact" varchar(36),
  "start_date" datetime,
  "end_date" datetime,
  "expected_completion_date" datetime,
  "actual_completion_date" datetime,
  "estimated_hours" INTEGER,
  "actual_hours" INTEGER,
  "category" TEXT,
  "type" TEXT
);

CREATE TABLE IF NOT EXISTS "telecom_room" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "building_id" varchar(36),
  "name" TEXT,
  "room_number" TEXT,
  "floor_number" TEXT,
  "room_type" TEXT,
  "description" TEXT,
  "power_available" numeric,
  "power_capacity_kw" REAL,
  "power_used_kw" REAL,
  "power_redundancy" TEXT,
  "ups_available" numeric,
  "ups_capacity_kva" REAL,
  "ups_runtime_minutes" INTEGER,
  "generator_available" numeric,
  "generator_capacity_kw" REAL,
  "hvac_available" numeric,
  "hvac_capacity_tons" REAL,
  "hvac_redundancy" TEXT,
  "cooling_type" TEXT,
  "temperature_monitoring" numeric,
  "humidity_monitoring" numeric,
  "number_of_racks" INTEGER,
  "rack_capacity_u" INTEGER,
  "floor_space_sqm" REAL,
  "ceiling_height_m" REAL,
  "raised_floor" numeric,
  "access_control" TEXT,
  "security_cameras" numeric,
  "access_log_system" numeric,
  "structured_cabling" numeric,
  "fiber_available" numeric,
  "copper_available" numeric,
  "cable_trays" numeric,
  "cable_management" TEXT,
  "fire_suppression" TEXT,
  "smoke_detection" numeric,
  "water_detection" numeric,
  "emergency_lighting" numeric,
  "emergency_power_off" numeric,
  "environmental_monitoring" numeric,
  "alarm_system" numeric,
  "remote_monitoring" numeric,
  "status" TEXT,
  "notes" TEXT
);

CREATE TABLE IF NOT EXISTS "telecom_room_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "telecom_room_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "tokens" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "user_id" varchar(36),
  "token" TEXT,
  "expires_at" datetime
);

CREATE TABLE IF NOT EXISTS "transport_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "transport_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "transport_vendor" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "transport_id" varchar(36),
  "name" TEXT,
  "poc" TEXT,
  "phone" TEXT,
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "transports" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "long_name" TEXT,
  "description" TEXT,
  "note" TEXT,
  "vendor" TEXT,
  "support" TEXT,
  "start_date" datetime,
  "end_date" datetime,
  "status" TEXT,
  "transport_type" TEXT,
  "service_type" TEXT,
  "carrier_id" TEXT,
  "contract_information" TEXT,
  "support_escalation" TEXT
);

CREATE TABLE IF NOT EXISTS "unplannedevent_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "unplanned_event_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE TABLE IF NOT EXISTS "unplannedevent_location" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "unplanned_event_id" varchar(36),
  "location_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "unplannedevent_network" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "unplanned_event_id" varchar(36),
  "network_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "unplannedevent_service" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "unplanned_event_id" varchar(36),
  "service_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "unplannedevent_updates" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "unplannedid" varchar(36),
  "updatedby" TEXT,
  "status" INTEGER,
  "createdonnetwork" TEXT,
  "createdbyuser" TEXT,
  "english" TEXT,
  "french" TEXT,
  "username" TEXT,
  "type" TEXT,
  "time" INTEGER
);

CREATE TABLE IF NOT EXISTS "unplannedevents" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "status" INTEGER,
  "updatedby" TEXT,
  "createdbyuser" TEXT,
  "legacyid" TEXT,
  "assignedto" varchar(36),
  "national" TEXT,
  "problem" TEXT,
  "cir" TEXT,
  "title" TEXT,
  "title_french" TEXT,
  "eventype" TEXT,
  "initial_id" varchar(36),
  "techimpact_id" varchar(36),
  "master_ticket" TEXT,
  "tickets" TEXT,
  "level" TEXT,
  "ima" TEXT,
  "ima_number" TEXT,
  "oia" varchar(36),
  "etr" TEXT,
  "create_time" INTEGER,
  "start_time" INTEGER,
  "resolved_time" INTEGER,
  "root_cause_found_time" INTEGER,
  "disa" TEXT,
  "ims" TEXT,
  "ecd" TEXT,
  "initial_statement" TEXT,
  "initial_statement_french" TEXT,
  "technical_impact" TEXT,
  "technical_impact_french" TEXT,
  "all_services_affected" numeric,
  "all_networks_affected" numeric
);

CREATE TABLE IF NOT EXISTS "users" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "username" varchar(100),
  "email" varchar(255),
  "password" TEXT,
  "certificate_login_only" numeric,
  "dashboard_show_planned" numeric,
  "dashboard_show_unplanned" numeric,
  "dashboard_show_problem" numeric,
  "dashboard_show_legend" numeric,
  "dashboard_show_news" numeric,
  "dashboard_show_worldmap" numeric,
  "dashboard_show_clocks" numeric,
  "worldmap_show_country_labels" numeric,
  "worldmap_show_event_lists" numeric,
  "session_duration_minutes" INTEGER,
  "theme_preference" TEXT,
  "language" varchar(8),
  "nav_collapsed" numeric,
  "showed_delete_warning" numeric,
  "timeline_range_preset" varchar(16),
  "dashboard_planned_range_preset" varchar(16),
  "dashboard_unplanned_range_preset" varchar(16),
  "dashboard_problem_range_preset" varchar(16),
  "dashboard_config" TEXT,
  "api_key" varchar(128),
  "default_l1_id" varchar(36),
  "default_smc_id" varchar(36),
  "default_site_id" varchar(36),
  "default_smc_ids" TEXT,
  "default_site_ids" TEXT,
  "calendar_event_types" TEXT,
  "lastlogintime" INTEGER,
  "enabled" numeric
);

CREATE TABLE IF NOT EXISTS "users_group" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "user_id" TEXT,
  "group_id" TEXT
);

CREATE TABLE IF NOT EXISTS "users_smc" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "user_id" varchar(36),
  "smc_id" varchar(36)
);

CREATE TABLE IF NOT EXISTS "usersreport" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "reporter_name" TEXT,
  "reporter_email" TEXT,
  "reporter_ip" TEXT,
  "office_phone" TEXT,
  "mobile_phone" TEXT,
  "description" TEXT,
  "network_id" TEXT,
  "service_id" TEXT,
  "site_id" TEXT,
  "custom_location" TEXT,
  "smc_id" TEXT,
  "l1_id" TEXT,
  "smc_name" TEXT,
  "l1_name" TEXT,
  "status" TEXT
);

CREATE TABLE IF NOT EXISTS "vrf" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "name" TEXT,
  "description" TEXT
);

CREATE TABLE IF NOT EXISTS "wifi" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "site_id" varchar(36),
  "name" TEXT,
  "ss_id" TEXT,
  "password" TEXT,
  "network" TEXT,
  "technology" TEXT,
  "description" TEXT,
  "classification" TEXT,
  "contact_id" varchar(36),
  "coverage_area" TEXT,
  "type" TEXT,
  "type_details" TEXT,
  "vendor" TEXT,
  "model" TEXT,
  "vendor_contact" TEXT,
  "vendor_phone" TEXT,
  "vendor_email" TEXT,
  "ap_count" INTEGER,
  "allow_byod" numeric,
  "bandwidth_up_limit_mbps" INTEGER,
  "bandwidth_down_limit_mbps" INTEGER,
  "wi_fi_standard" TEXT,
  "security_type" TEXT,
  "frequency_band" TEXT,
  "channel" TEXT,
  "bandwidth" TEXT,
  "max_speed" INTEGER,
  "authentication_method" TEXT,
  "radius_server" TEXT,
  "encryption" TEXT,
  "hidden_ss_id" numeric
);

CREATE TABLE IF NOT EXISTS "wifi_contact" (
  "id" varchar(36) PRIMARY KEY,
  "created_time" datetime,
  "updated_time" datetime,
  "deleted_at" INTEGER,
  "wi_fi_id" varchar(36),
  "contact_id" varchar(36),
  "note" TEXT
);

CREATE INDEX IF NOT EXISTS `idx_apikey_created_by` ON `apikey`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_apikey_deleted_at` ON `apikey`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_apikey_expires_at` ON `apikey`(`expires_at`);
CREATE INDEX IF NOT EXISTS `idx_apikey_group_api_key_id` ON `apikey_group`(`api_key_id`);
CREATE INDEX IF NOT EXISTS `idx_apikey_group_deleted_at` ON `apikey_group`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_apikey_group_group_id` ON `apikey_group`(`group_id`);
CREATE INDEX IF NOT EXISTS `idx_apikey_is_active` ON `apikey`(`is_active`);
CREATE INDEX IF NOT EXISTS `idx_apikey_key_hash` ON `apikey`(`key_hash`);
CREATE INDEX IF NOT EXISTS `idx_apikey_last_used` ON `apikey`(`last_used`);
CREATE INDEX IF NOT EXISTS `idx_apikey_name` ON `apikey`(`name`);
CREATE INDEX IF NOT EXISTS `idx_assistant_conversations_deleted_at` ON `assistant_conversations`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_assistant_conversations_updated_at` ON `assistant_conversations`(`updated_at`);
CREATE INDEX IF NOT EXISTS `idx_assistant_conversations_user_id` ON `assistant_conversations`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_assistant_message_order` ON `assistant_messages`(`conversation_id`,`created_at_unix`);
CREATE INDEX IF NOT EXISTS `idx_assistant_messages_deleted_at` ON `assistant_messages`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_assistant_tool_calls_conversation_id` ON `assistant_tool_calls`(`conversation_id`);
CREATE INDEX IF NOT EXISTS `idx_assistant_tool_calls_deleted_at` ON `assistant_tool_calls`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_assistant_tool_calls_expires_at` ON `assistant_tool_calls`(`expires_at`);
CREATE INDEX IF NOT EXISTS `idx_assistant_tool_calls_status` ON `assistant_tool_calls`(`status`);
CREATE INDEX IF NOT EXISTS `idx_assistant_tool_calls_user_id` ON `assistant_tool_calls`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_br_approval_date` ON `br`(`approval_date`);
CREATE INDEX IF NOT EXISTS `idx_br_br_phase_id` ON `br`(`br_phase_id`);
CREATE INDEX IF NOT EXISTS `idx_br_br_status_id` ON `br`(`br_status_id`);
CREATE INDEX IF NOT EXISTS `idx_br_br_type_id` ON `br`(`br_type_id`);
CREATE INDEX IF NOT EXISTS idx_br_brbits_number ON br(brbits_number);
CREATE INDEX IF NOT EXISTS `idx_br_contact_br_id` ON `br_contact`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_br_contact_contact_id` ON `br_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_br_contact_deleted_at` ON `br_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_deleted_at` ON `br`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_documents_br_id` ON `br_documents`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_br_documents_deleted_at` ON `br_documents`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_documents_document_type` ON `br_documents`(`document_type`);
CREATE INDEX IF NOT EXISTS `idx_br_finance_br_id` ON `br_finance`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_br_finance_deleted_at` ON `br_finance`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_finance_funding_contact` ON `br_finance`(`funding_contact`);
CREATE INDEX IF NOT EXISTS `idx_br_l1_id` ON `br`(`l1_id`);
CREATE INDEX IF NOT EXISTS `idx_br_name` ON `br`(`name`);
CREATE INDEX IF NOT EXISTS `idx_br_phase_deleted_at` ON `br_phase`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_phase_name` ON `br_phase`(`name`);
CREATE INDEX IF NOT EXISTS `idx_br_project_br_id` ON `br_project`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_br_project_deleted_at` ON `br_project`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_project_project_id` ON `br_project`(`project_id`);
CREATE INDEX IF NOT EXISTS `idx_br_sd_br_id` ON `br_sd`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_br_sd_deleted_at` ON `br_sd`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_br_site_id` ON `br`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_br_site_status ON br(site_id, status);
CREATE INDEX IF NOT EXISTS `idx_br_status` ON `br`(`status`);
CREATE INDEX IF NOT EXISTS `idx_br_status_deleted_at` ON `br_status`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_br_status_description ON br_status(description);
CREATE INDEX IF NOT EXISTS idx_br_status_french_description ON br_status(french_description);
CREATE INDEX IF NOT EXISTS idx_br_status_french_name ON br_status(french_name);
CREATE INDEX IF NOT EXISTS `idx_br_status_name` ON `br_status`(`name`);
CREATE INDEX IF NOT EXISTS idx_br_status_submit ON br(status, submit_date);
CREATE INDEX IF NOT EXISTS `idx_br_submit_date` ON `br`(`submit_date`);
CREATE INDEX IF NOT EXISTS `idx_br_supportive_smc_id` ON `br`(`supportive_smc_id`);
CREATE INDEX IF NOT EXISTS `idx_br_type_deleted_at` ON `br_type`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_br_type_description ON br_type(description);
CREATE INDEX IF NOT EXISTS idx_br_type_french_description ON br_type(french_description);
CREATE INDEX IF NOT EXISTS idx_br_type_french_name ON br_type(french_name);
CREATE INDEX IF NOT EXISTS `idx_br_type_name` ON `br_type`(`name`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_building_id` ON `building_telecom_room`(`building_id`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_contact_id` ON `building_telecom_room`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_deleted_at` ON `building_telecom_room`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_floor_number` ON `building_telecom_room`(`floor_number`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_room_type` ON `building_telecom_room`(`room_type`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_site_id` ON `building_telecom_room`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_building_telecom_room_status` ON `building_telecom_room`(`status`);
CREATE INDEX IF NOT EXISTS `idx_buildings_ao_house_num` ON `buildings`(`ao_house_num`);
CREATE INDEX IF NOT EXISTS `idx_buildings_building_manager_contact_id` ON `buildings`(`building_manager_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_buildings_building_number` ON `buildings`(`building_number`);
CREATE INDEX IF NOT EXISTS `idx_buildings_city` ON `buildings`(`city`);
CREATE INDEX IF NOT EXISTS `idx_buildings_deleted_at` ON `buildings`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_buildings_emergency_contact_id` ON `buildings`(`emergency_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_buildings_has_datacenter` ON `buildings`(`has_datacenter`);
CREATE INDEX IF NOT EXISTS `idx_buildings_is_mdf` ON `buildings`(`is_mdf`);
CREATE INDEX IF NOT EXISTS `idx_buildings_it_contact_id` ON `buildings`(`it_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_buildings_name` ON `buildings`(`name`);
CREATE INDEX IF NOT EXISTS idx_buildings_number_of_floors ON buildings(number_of_floors);
CREATE INDEX IF NOT EXISTS idx_buildings_site_city ON buildings(site_id, city);
CREATE INDEX IF NOT EXISTS `idx_buildings_site_id` ON `buildings`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_buildings_street_name` ON `buildings`(`street_name`);
CREATE INDEX IF NOT EXISTS `idx_cbas_acronym` ON `cbas`(`acronym`);
CREATE INDEX IF NOT EXISTS `idx_cbas_apm_id` ON `cbas`(`apm_id`);
CREATE INDEX IF NOT EXISTS `idx_cbas_deleted_at` ON `cbas`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_cbas_departmental_application_id` ON `cbas`(`departmental_application_id`);
CREATE INDEX IF NOT EXISTS `idx_cbas_ecs` ON `cbas`(`ecs`);
CREATE INDEX IF NOT EXISTS `idx_cbas_ecsl1` ON `cbas`(`ecsl1`);
CREATE INDEX IF NOT EXISTS `idx_cbas_l1` ON `cbas`(`l1`);
CREATE INDEX IF NOT EXISTS `idx_cbas_lifecycle` ON `cbas`(`lifecycle`);
CREATE INDEX IF NOT EXISTS `idx_cbas_name` ON `cbas`(`name`);
CREATE INDEX IF NOT EXISTS `idx_cbas_network` ON `cbas`(`network`);
CREATE INDEX IF NOT EXISTS `idx_chat_room_time` ON `gorm_chat_messages`(`room`,`time`);
CREATE INDEX IF NOT EXISTS `idx_contacts_deleted_at` ON `contacts`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_contacts_department` ON `contacts`(`department`);
CREATE INDEX IF NOT EXISTS `idx_contacts_l1_id` ON `contacts`(`l1_id`);
CREATE INDEX IF NOT EXISTS idx_contacts_l1_name ON contacts(l1_id, name);
CREATE INDEX IF NOT EXISTS `idx_contacts_location` ON `contacts`(`location`);
CREATE INDEX IF NOT EXISTS `idx_contacts_name` ON `contacts`(`name`);
CREATE INDEX IF NOT EXISTS idx_contacts_organization ON contacts(organization);
CREATE INDEX IF NOT EXISTS idx_contacts_rank ON contacts(rank);
CREATE INDEX IF NOT EXISTS `idx_contacts_status` ON `contacts`(`status`);
CREATE INDEX IF NOT EXISTS `idx_departments_deleted_at` ON `departments`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_departments_description ON departments(description);
CREATE INDEX IF NOT EXISTS idx_departments_french_description ON departments(french_description);
CREATE INDEX IF NOT EXISTS idx_departments_french_name ON departments(french_name);
CREATE INDEX IF NOT EXISTS `idx_departments_name` ON `departments`(`name`);
CREATE INDEX IF NOT EXISTS `idx_device_contact_contact_id` ON `device_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_device_contact_deleted_at` ON `device_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_device_contact_device_id` ON `device_contact`(`device_id`);
CREATE INDEX IF NOT EXISTS `idx_devices_building_id` ON `devices`(`building_id`);
CREATE INDEX IF NOT EXISTS `idx_devices_deleted_at` ON `devices`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_devices_model` ON `devices`(`model`);
CREATE INDEX IF NOT EXISTS `idx_devices_name` ON `devices`(`name`);
CREATE INDEX IF NOT EXISTS `idx_devices_network_id` ON `devices`(`network_id`);
CREATE INDEX IF NOT EXISTS `idx_devices_service_id` ON `devices`(`service_id`);
CREATE INDEX IF NOT EXISTS `idx_devices_site_id` ON `devices`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_devices_telecom_room_id` ON `devices`(`telecom_room_id`);
CREATE INDEX IF NOT EXISTS `idx_devices_type` ON `devices`(`type`);
CREATE INDEX IF NOT EXISTS `idx_devices_vendor` ON `devices`(`vendor`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_created_by` ON `distribution_lists`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_deleted_at` ON `distribution_lists`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_email_address` ON `distribution_lists`(`email_address`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_name` ON `distribution_lists`(`name`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_cir` ON `distribution_lists`(`send_cir`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_ima` ON `distribution_lists`(`send_ima`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_approved` ON `distribution_lists`(`send_planned_approved`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_cancelled` ON `distribution_lists`(`send_planned_cancelled`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_end` ON `distribution_lists`(`send_planned_end`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_pending` ON `distribution_lists`(`send_planned_pending`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_rescheduled` ON `distribution_lists`(`send_planned_rescheduled`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_start` ON `distribution_lists`(`send_planned_start`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_planned_submitted` ON `distribution_lists`(`send_planned_submitted`);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_send_unplanned` ON `distribution_lists`(`send_unplanned`);
CREATE INDEX IF NOT EXISTS idx_distribution_lists_smc_ids ON distribution_lists(smc_ids);
CREATE INDEX IF NOT EXISTS `idx_distribution_lists_type` ON `distribution_lists`(`type`);
CREATE INDEX IF NOT EXISTS `idx_document_br_br_id` ON `document_br`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_document_br_deleted_at` ON `document_br`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_document_br_document_id` ON `document_br`(`document_id`);
CREATE INDEX IF NOT EXISTS `idx_document_contact_contact_id` ON `document_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_document_contact_deleted_at` ON `document_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_document_contact_document_id` ON `document_contact`(`document_id`);
CREATE INDEX IF NOT EXISTS `idx_document_project_deleted_at` ON `document_project`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_document_project_document_id` ON `document_project`(`document_id`);
CREATE INDEX IF NOT EXISTS `idx_document_project_project_id` ON `document_project`(`project_id`);
CREATE INDEX IF NOT EXISTS `idx_document_site_deleted_at` ON `document_site`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_document_site_document_id` ON `document_site`(`document_id`);
CREATE INDEX IF NOT EXISTS `idx_document_site_site_id` ON `document_site`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_documents_br_id` ON `documents`(`br_id`);
CREATE INDEX IF NOT EXISTS idx_documents_classification ON documents(classification);
CREATE INDEX IF NOT EXISTS `idx_documents_contact_id` ON `documents`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_documents_content_sha256` ON `documents`(`content_sha256`);
CREATE INDEX IF NOT EXISTS `idx_documents_created_by` ON `documents`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_documents_deleted_at` ON `documents`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_documents_site_contact ON documents(site_id, contact_id);
CREATE INDEX IF NOT EXISTS `idx_documents_site_id` ON `documents`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_documents_updated_time ON documents(updated_time);
CREATE INDEX IF NOT EXISTS `idx_gorm_chat_messages_msg_id` ON `gorm_chat_messages`(`msg_id`);
CREATE INDEX IF NOT EXISTS `idx_gorm_chat_messages_user_id` ON `gorm_chat_messages`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_group_permissions_collection` ON `group_permissions`(`table_name`);
CREATE INDEX IF NOT EXISTS `idx_group_permissions_deleted_at` ON `group_permissions`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_group_permissions_group_id` ON `group_permissions`(`group_id`);
CREATE INDEX IF NOT EXISTS idx_group_permissions_group_table ON group_permissions(group_id, table_name);
CREATE INDEX IF NOT EXISTS `idx_groups_deleted_at` ON `groups`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_groups_description ON groups(description);
CREATE INDEX IF NOT EXISTS `idx_groups_name` ON `groups`(`name`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_connected_to_building_id` ON `interfaces`(`connected_to_building_id`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_connected_to_device_id` ON `interfaces`(`connected_to_device_id`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_connected_to_site_id` ON `interfaces`(`connected_to_site_id`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_deleted_at` ON `interfaces`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_device_id` ON `interfaces`(`device_id`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_ipv4` ON `interfaces`(`ipv4`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_ipv6` ON `interfaces`(`ipv6`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_macd` ON `interfaces`(`macd`);
CREATE INDEX IF NOT EXISTS `idx_interfaces_to_interface_id` ON `interfaces`(`to_interface_id`);
CREATE INDEX IF NOT EXISTS `idx_issue_updates_created_by` ON `issue_updates`(`created_by`);
CREATE INDEX IF NOT EXISTS idx_issue_updates_created_time ON issue_updates(created_time);
CREATE INDEX IF NOT EXISTS `idx_issue_updates_deleted_at` ON `issue_updates`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_issue_updates_issue_id` ON `issue_updates`(`issue_id`);
CREATE INDEX IF NOT EXISTS `idx_issue_updates_status` ON `issue_updates`(`status`);
CREATE INDEX IF NOT EXISTS `idx_issues_assigned_to_contact` ON `issues`(`assigned_to_contact`);
CREATE INDEX IF NOT EXISTS `idx_issues_assigned_to_l1` ON `issues`(`assigned_to_l1`);
CREATE INDEX IF NOT EXISTS `idx_issues_category` ON `issues`(`category`);
CREATE INDEX IF NOT EXISTS `idx_issues_created_by` ON `issues`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_issues_deleted_at` ON `issues`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_issues_end_date` ON `issues`(`end_date`);
CREATE INDEX IF NOT EXISTS `idx_issues_expected_resolution_date` ON `issues`(`expected_resolution_date`);
CREATE INDEX IF NOT EXISTS `idx_issues_priority` ON `issues`(`priority`);
CREATE INDEX IF NOT EXISTS `idx_issues_reported_date` ON `issues`(`reported_date`);
CREATE INDEX IF NOT EXISTS `idx_issues_severity` ON `issues`(`severity`);
CREATE INDEX IF NOT EXISTS `idx_issues_site_id` ON `issues`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_issues_site_priority ON issues(site_id, priority);
CREATE INDEX IF NOT EXISTS idx_issues_site_status ON issues(site_id, status);
CREATE INDEX IF NOT EXISTS `idx_issues_start_date` ON `issues`(`start_date`);
CREATE INDEX IF NOT EXISTS `idx_issues_status` ON `issues`(`status`);
CREATE INDEX IF NOT EXISTS idx_issues_title ON issues(title);
CREATE INDEX IF NOT EXISTS `idx_issues_type` ON `issues`(`type`);
CREATE INDEX IF NOT EXISTS `idx_l1_deleted_at` ON `l1`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_l1_description ON l1(description);
CREATE INDEX IF NOT EXISTS idx_l1_french_description ON l1(french_description);
CREATE INDEX IF NOT EXISTS idx_l1_french_long_name ON l1(french_long_name);
CREATE INDEX IF NOT EXISTS idx_l1_french_name ON l1(french_name);
CREATE INDEX IF NOT EXISTS idx_l1_long_name ON l1(long_name);
CREATE INDEX IF NOT EXISTS `idx_l1_name` ON `l1`(`name`);
CREATE INDEX IF NOT EXISTS idx_lias_bandwidth_down_mbps ON lias(bandwidth_down_mbps);
CREATE INDEX IF NOT EXISTS `idx_lias_billing_account_number` ON `lias`(`billing_account_number`);
CREATE INDEX IF NOT EXISTS `idx_lias_circuit_id` ON `lias`(`circuit_id`);
CREATE INDEX IF NOT EXISTS `idx_lias_city_province` ON `lias`(`city`,`province_code`);
CREATE INDEX IF NOT EXISTS `idx_lias_contact_contact_id` ON `lias_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_lias_contact_deleted_at` ON `lias_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_lias_contact_id` ON `lias`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_lias_contact_lias_id` ON `lias_contact`(`lias_id`);
CREATE INDEX IF NOT EXISTS `idx_lias_contract_end_date` ON `lias`(`contract_end_date`);
CREATE INDEX IF NOT EXISTS `idx_lias_contract_number` ON `lias`(`contract_number`);
CREATE INDEX IF NOT EXISTS `idx_lias_deleted_at` ON `lias`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_lias_l1_id` ON `lias`(`l1_id`);
CREATE INDEX IF NOT EXISTS idx_lias_monthly_cost ON lias(monthly_cost);
CREATE INDEX IF NOT EXISTS `idx_lias_secondary_circuit_id` ON `lias`(`secondary_circuit_id`);
CREATE INDEX IF NOT EXISTS `idx_lias_service_provider` ON `lias`(`service_provider`);
CREATE INDEX IF NOT EXISTS `idx_lias_service_start_date` ON `lias`(`service_start_date`);
CREATE INDEX IF NOT EXISTS `idx_lias_site_id` ON `lias`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_lias_site_vendor ON lias(site_id, vendor);
CREATE INDEX IF NOT EXISTS `idx_lias_source_last_update_date` ON `lias`(`source_last_update_date`);
CREATE INDEX IF NOT EXISTS `idx_lias_source_record_id` ON `lias`(`source_record_id`);
CREATE INDEX IF NOT EXISTS `idx_lias_status` ON `lias`(`status`);
CREATE INDEX IF NOT EXISTS `idx_lias_vendor` ON `lias`(`vendor`);
CREATE INDEX IF NOT EXISTS `idx_networks_deleted_at` ON `networks`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_networks_description` ON `networks`(`description`);
CREATE INDEX IF NOT EXISTS idx_networks_french_description ON networks(french_description);
CREATE INDEX IF NOT EXISTS idx_networks_french_name ON networks(french_name);
CREATE INDEX IF NOT EXISTS `idx_networks_name` ON `networks`(`name`);
CREATE INDEX IF NOT EXISTS idx_news_active_priority ON news(is_active, priority);
CREATE INDEX IF NOT EXISTS `idx_news_created_by` ON `news`(`created_by`);
CREATE INDEX IF NOT EXISTS idx_news_created_by_time ON news(created_by, created_time);
CREATE INDEX IF NOT EXISTS `idx_news_deleted_at` ON `news`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_news_is_active` ON `news`(`is_active`);
CREATE INDEX IF NOT EXISTS `idx_news_is_national` ON `news`(`is_national`);
CREATE INDEX IF NOT EXISTS idx_news_label ON news(label);
CREATE INDEX IF NOT EXISTS `idx_news_priority` ON `news`(`priority`);
CREATE INDEX IF NOT EXISTS `idx_news_show_end_time` ON `news`(`show_end_time`);
CREATE INDEX IF NOT EXISTS `idx_news_show_start_time` ON `news`(`show_start_time`);
CREATE INDEX IF NOT EXISTS idx_news_title ON news(title);
CREATE INDEX IF NOT EXISTS `idx_oia_deleted_at` ON `oia`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_oia_french_description ON oia(french_description);
CREATE INDEX IF NOT EXISTS idx_oia_french_name ON oia(french_name);
CREATE INDEX IF NOT EXISTS `idx_oia_name` ON `oia`(`name`);
CREATE INDEX IF NOT EXISTS `idx_planned_authority_deleted_at` ON `planned_authority`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_planned_authority_french_description ON planned_authority(french_description);
CREATE INDEX IF NOT EXISTS idx_planned_authority_french_name ON planned_authority(french_name);
CREATE INDEX IF NOT EXISTS `idx_planned_authority_name` ON `planned_authority`(`name`);
CREATE INDEX IF NOT EXISTS `idx_planned_authority_user_authority_id` ON `planned_authority_user`(`authority_id`);
CREATE INDEX IF NOT EXISTS `idx_planned_authority_user_deleted_at` ON `planned_authority_user`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_planned_authority_user_unique ON planned_authority_user(authority_id, user_id);
CREATE INDEX IF NOT EXISTS `idx_planned_authority_user_user_id` ON `planned_authority_user`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_planned_comments_authority_id` ON `planned_comments`(`authority_id`);
CREATE INDEX IF NOT EXISTS `idx_planned_comments_created_by` ON `planned_comments`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_planned_comments_deleted_at` ON `planned_comments`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_planned_comments_event ON planned_comments(planned_event_id, created_time);
CREATE INDEX IF NOT EXISTS `idx_planned_comments_planned_event_id` ON `planned_comments`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS idx_planned_vote_unique ON planned_votes(planned_event_id, authority_id);
CREATE INDEX IF NOT EXISTS `idx_planned_votes_authority_id` ON `planned_votes`(`authority_id`);
CREATE INDEX IF NOT EXISTS `idx_planned_votes_deleted_at` ON `planned_votes`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_planned_votes_planned_event_id` ON `planned_votes`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_planned_votes_user_id` ON `planned_votes`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_contact_contact_id` ON `plannedevent_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_contact_deleted_at` ON `plannedevent_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_plannedevent_contact_event_id ON plannedevent_contact(planned_event_id);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_contact_planned_event_id` ON `plannedevent_contact`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_location_deleted_at` ON `plannedevent_location`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_location_location_id` ON `plannedevent_location`(`location_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_location_planned_event_id` ON `plannedevent_location`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_manual_occurrence_deleted_at` ON `plannedevent_manual_occurrence`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_manual_occurrence_occurrence_time` ON `plannedevent_manual_occurrence`(`occurrence_time`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_manual_occurrence_planned_event_id` ON `plannedevent_manual_occurrence`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_network_deleted_at` ON `plannedevent_network`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_network_network_id` ON `plannedevent_network`(`network_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_network_planned_event_id` ON `plannedevent_network`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_service_deleted_at` ON `plannedevent_service`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_service_planned_event_id` ON `plannedevent_service`(`planned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_plannedevent_service_service_id` ON `plannedevent_service`(`service_id`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_active_legacyid_unique ON plannedevents(legacyid) WHERE deleted_at IS NULL AND TRIM(legacyid) <> '';
CREATE INDEX IF NOT EXISTS `idx_plannedevents_assigned_to` ON `plannedevents`(`assignedto`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_assignedto ON plannedevents(assignedto);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_created_by_user` ON `plannedevents`(`createdbyuser`);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_deleted_at` ON `plannedevents`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_end_time` ON `plannedevents`(`endtime`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_endtime ON plannedevents(endtime);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_legacy_id` ON `plannedevents`(`legacyid`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_live_rs_end ON plannedevents(runningstatus, endtime) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_plannedevents_live_rs_next_start ON plannedevents(runningstatus, next_event, starttime) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_plannedevents_live_rs_start ON plannedevents(runningstatus, starttime) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_plannedevents_live_running ON plannedevents(runningstatus) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS `idx_plannedevents_master_ticket` ON `plannedevents`(`master_ticket`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_national ON plannedevents(national);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_next_event` ON `plannedevents`(`next_event`);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_recurrent_parent_id` ON `plannedevents`(`recurrentparentid`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_rs_end ON plannedevents(runningstatus, endtime);
CREATE INDEX IF NOT EXISTS idx_plannedevents_rs_next ON plannedevents(runningstatus, next_event);
CREATE INDEX IF NOT EXISTS idx_plannedevents_rs_start ON plannedevents(runningstatus, starttime);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_running_status` ON `plannedevents`(`runningstatus`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_runningstatus ON plannedevents(runningstatus);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_start_time` ON `plannedevents`(`starttime`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_starttime ON plannedevents(starttime);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_status` ON `plannedevents`(`status`);
CREATE INDEX IF NOT EXISTS idx_plannedevents_title ON plannedevents(title);
CREATE INDEX IF NOT EXISTS `idx_plannedevents_type` ON `plannedevents`(`type`);
CREATE INDEX IF NOT EXISTS `idx_pls_application` ON `pls`(`application`);
CREATE INDEX IF NOT EXISTS `idx_pls_contact_id` ON `pls`(`contact_id`);
CREATE INDEX IF NOT EXISTS idx_pls_cost ON pls(cost);
CREATE INDEX IF NOT EXISTS `idx_pls_deleted_at` ON `pls`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_pls_finance_amount ON pls_finance(amount);
CREATE INDEX IF NOT EXISTS `idx_pls_finance_deleted_at` ON `pls_finance`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_pls_finance_funding_contact` ON `pls_finance`(`funding_contact`);
CREATE INDEX IF NOT EXISTS idx_pls_finance_fy ON pls_finance(fy);
CREATE INDEX IF NOT EXISTS `idx_pls_finance_pls_id` ON `pls_finance`(`pls_id`);
CREATE INDEX IF NOT EXISTS `idx_pls_site_a_contact_id` ON `pls`(`site_a_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_pls_site_a_site_id` ON `pls`(`site_a_site_id`);
CREATE INDEX IF NOT EXISTS idx_pls_site_application ON pls(site_id, application);
CREATE INDEX IF NOT EXISTS `idx_pls_site_b_contact_id` ON `pls`(`site_b_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_pls_site_b_site_id` ON `pls`(`site_b_site_id`);
CREATE INDEX IF NOT EXISTS `idx_pls_site_id` ON `pls`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_pls_speed_mbps ON pls(speed_mbps);
CREATE INDEX IF NOT EXISTS idx_pls_telecom ON pls(telecom);
CREATE INDEX IF NOT EXISTS idx_pls_vendor ON pls(vendor);
CREATE INDEX IF NOT EXISTS `idx_project_contact_contact_id` ON `project_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_project_contact_deleted_at` ON `project_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_project_contact_project_id` ON `project_contact`(`project_id`);
CREATE INDEX IF NOT EXISTS `idx_projects_budget` ON `projects`(`budget`);
CREATE INDEX IF NOT EXISTS `idx_projects_deleted_at` ON `projects`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_projects_end_date` ON `projects`(`end_date`);
CREATE INDEX IF NOT EXISTS `idx_projects_l1_id` ON `projects`(`l1_id`);
CREATE INDEX IF NOT EXISTS idx_projects_l1_status ON projects(l1_id, status);
CREATE INDEX IF NOT EXISTS `idx_projects_name` ON `projects`(`name`);
CREATE INDEX IF NOT EXISTS `idx_projects_project_number` ON `projects`(`project_number`);
CREATE INDEX IF NOT EXISTS `idx_projects_smc_id` ON `projects`(`smc_id`);
CREATE INDEX IF NOT EXISTS `idx_projects_sponsor_contact` ON `projects`(`sponsor_contact`);
CREATE INDEX IF NOT EXISTS `idx_projects_start_date` ON `projects`(`start_date`);
CREATE INDEX IF NOT EXISTS `idx_projects_status` ON `projects`(`status`);
CREATE INDEX IF NOT EXISTS idx_projects_status_start ON projects(status, start_date);
CREATE INDEX IF NOT EXISTS `idx_projects_type` ON `projects`(`type`);
CREATE INDEX IF NOT EXISTS `idx_queries_deleted_at` ON `queries`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_queries_time` ON `queries`(`time`);
CREATE INDEX IF NOT EXISTS `idx_queries_username` ON `queries`(`username`);
CREATE INDEX IF NOT EXISTS `idx_scn_application` ON `scn`(`application`);
CREATE INDEX IF NOT EXISTS `idx_scn_contact_id` ON `scn`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_scn_deleted_at` ON `scn`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_scn_finance_deleted_at` ON `scn_finance`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_scn_finance_funding_contact` ON `scn_finance`(`funding_contact`);
CREATE INDEX IF NOT EXISTS `idx_scn_finance_scn_id` ON `scn_finance`(`scn_id`);
CREATE INDEX IF NOT EXISTS `idx_scn_site_a_contact_id` ON `scn`(`site_a_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_scn_site_a_site_id` ON `scn`(`site_a_site_id`);
CREATE INDEX IF NOT EXISTS `idx_scn_site_b_contact_id` ON `scn`(`site_b_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_scn_site_b_site_id` ON `scn`(`site_b_site_id`);
CREATE INDEX IF NOT EXISTS `idx_scn_site_id` ON `scn`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_sdp_transport_deleted_at` ON `sdp_transport`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_sdp_transport_sdp_id` ON `sdp_transport`(`sdp_id`);
CREATE INDEX IF NOT EXISTS `idx_sdp_transport_transport_id` ON `sdp_transport`(`transport_id`);
CREATE INDEX IF NOT EXISTS `idx_sdp_vrf_deleted_at` ON `sdp_vrf`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_sdp_vrf_sdp_id` ON `sdp_vrf`(`sdp_id`);
CREATE INDEX IF NOT EXISTS `idx_sdp_vrf_vrf_id` ON `sdp_vrf`(`vrf_id`);
CREATE INDEX IF NOT EXISTS `idx_servicelines_deleted_at` ON `servicelines`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_servicelines_department_id` ON `servicelines`(`department_id`);
CREATE INDEX IF NOT EXISTS `idx_servicelines_l1_id` ON `servicelines`(`l1_id`);
CREATE INDEX IF NOT EXISTS `idx_servicelines_manager_contact_id` ON `servicelines`(`manager_contact_id`);
CREATE INDEX IF NOT EXISTS `idx_servicelines_name` ON `servicelines`(`name`);
CREATE INDEX IF NOT EXISTS `idx_services_deleted_at` ON `services`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_services_description` ON `services`(`description`);
CREATE INDEX IF NOT EXISTS idx_services_french_description ON services(french_description);
CREATE INDEX IF NOT EXISTS idx_services_french_name ON services(french_name);
CREATE INDEX IF NOT EXISTS `idx_services_name` ON `services`(`name`);
CREATE INDEX IF NOT EXISTS `idx_sessions_deleted_at` ON `sessions`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_sessions_expires_at` ON `sessions`(`expires_at`);
CREATE INDEX IF NOT EXISTS `idx_sessions_login_time` ON `sessions`(`login_time`);
CREATE INDEX IF NOT EXISTS `idx_sessions_session_id` ON `sessions`(`session_id`);
CREATE INDEX IF NOT EXISTS `idx_sessions_username` ON `sessions`(`username`);
CREATE INDEX IF NOT EXISTS idx_sessions_username_expires ON sessions(username, expires_at);
CREATE INDEX IF NOT EXISTS `idx_site_access_type_deleted_at` ON `site_access_type`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_site_access_type_french_description ON site_access_type(french_description);
CREATE INDEX IF NOT EXISTS idx_site_access_type_french_name ON site_access_type(french_name);
CREATE INDEX IF NOT EXISTS `idx_site_access_type_name` ON `site_access_type`(`name`);
CREATE INDEX IF NOT EXISTS `idx_site_br_br_id` ON `site_br`(`br_id`);
CREATE INDEX IF NOT EXISTS `idx_site_br_deleted_at` ON `site_br`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_site_br_site_id` ON `site_br`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_site_contact_contact_id` ON `site_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_site_contact_deleted_at` ON `site_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_site_contact_site_id` ON `site_contact`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_site_project_deleted_at` ON `site_project`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_site_project_project_id` ON `site_project`(`project_id`);
CREATE INDEX IF NOT EXISTS `idx_site_project_site_id` ON `site_project`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_site_sdp_deleted_at` ON `site_sdp`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_site_sdp_site_id` ON `site_sdp`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_site_sdp_transport_id` ON `site_sdp`(`transport_id`);
CREATE INDEX IF NOT EXISTS `idx_sites_cbas` ON `sites`(`cbas`);
CREATE INDEX IF NOT EXISTS `idx_sites_city` ON `sites`(`city`);
CREATE INDEX IF NOT EXISTS idx_sites_city_country ON sites(city, country);
CREATE INDEX IF NOT EXISTS idx_sites_class ON sites(class);
CREATE INDEX IF NOT EXISTS `idx_sites_country` ON `sites`(`country`);
CREATE INDEX IF NOT EXISTS idx_sites_created_time ON sites(created_time DESC);
CREATE INDEX IF NOT EXISTS `idx_sites_deleted_at` ON `sites`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_sites_deployed` ON `sites`(`deployed`);
CREATE INDEX IF NOT EXISTS idx_sites_deployed_status ON sites(deployed, status);
CREATE INDEX IF NOT EXISTS `idx_sites_l1_id` ON `sites`(`l1_id`);
CREATE INDEX IF NOT EXISTS idx_sites_l1_status ON sites(l1_id, status);
CREATE INDEX IF NOT EXISTS `idx_sites_name` ON `sites`(`name`);
CREATE INDEX IF NOT EXISTS idx_sites_sda_id ON sites(sda_id);
CREATE INDEX IF NOT EXISTS `idx_sites_site_access_type_id` ON `sites`(`site_access_type_id`);
CREATE INDEX IF NOT EXISTS `idx_sites_smc_id` ON `sites`(`smc_id`);
CREATE INDEX IF NOT EXISTS idx_sites_smc_status ON sites(smc_id, status);
CREATE INDEX IF NOT EXISTS `idx_sites_ssc_region_id` ON `sites`(`ssc_region_id`);
CREATE INDEX IF NOT EXISTS `idx_sites_status` ON `sites`(`status`);
CREATE INDEX IF NOT EXISTS `idx_smc_contact_contact_id` ON `smc_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_smc_contact_deleted_at` ON `smc_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_smc_contact_smc_id` ON `smc_contact`(`smc_id`);
CREATE INDEX IF NOT EXISTS `idx_smc_deleted_at` ON `smc`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_smc_l1_id` ON `smc`(`l1_id`);
CREATE INDEX IF NOT EXISTS `idx_smc_name` ON `smc`(`name`);
CREATE INDEX IF NOT EXISTS `idx_ssc_region_deleted_at` ON `ssc_region`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_ssc_region_french_description ON ssc_region(french_description);
CREATE INDEX IF NOT EXISTS idx_ssc_region_french_name ON ssc_region(french_name);
CREATE INDEX IF NOT EXISTS `idx_ssc_region_name` ON `ssc_region`(`name`);
CREATE INDEX IF NOT EXISTS `idx_stats_daily_collection` ON `stats_daily`(`collection`);
CREATE INDEX IF NOT EXISTS `idx_stats_daily_deleted_at` ON `stats_daily`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_stats_daily_endpoint` ON `stats_daily`(`endpoint`);
CREATE INDEX IF NOT EXISTS `idx_stats_daily_unique` ON `stats_daily`(`day_timestamp`,`endpoint`,`method`,`collection`,`operation`,`status_code`);
CREATE INDEX IF NOT EXISTS `idx_stats_event_collection` ON `stats_events`(`collection`);
CREATE INDEX IF NOT EXISTS `idx_stats_event_endpoint` ON `stats_events`(`endpoint`);
CREATE INDEX IF NOT EXISTS `idx_stats_event_status` ON `stats_events`(`status_code`);
CREATE INDEX IF NOT EXISTS `idx_stats_event_time` ON `stats_events`(`timestamp`);
CREATE INDEX IF NOT EXISTS `idx_stats_event_user` ON `stats_events`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_stats_events_deleted_at` ON `stats_events`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_stats_hourly_collection` ON `stats_hourly`(`collection`);
CREATE INDEX IF NOT EXISTS `idx_stats_hourly_deleted_at` ON `stats_hourly`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_stats_hourly_endpoint` ON `stats_hourly`(`endpoint`);
CREATE INDEX IF NOT EXISTS `idx_stats_hourly_status` ON `stats_hourly`(`status_code`);
CREATE INDEX IF NOT EXISTS `idx_stats_hourly_unique` ON `stats_hourly`(`hour_timestamp`,`endpoint`,`method`,`collection`,`operation`,`status_code`);
CREATE INDEX IF NOT EXISTS `idx_stats_slow_collection` ON `stats_slow_queries`(`collection`);
CREATE INDEX IF NOT EXISTS `idx_stats_slow_duration` ON `stats_slow_queries`(`duration_ms`);
CREATE INDEX IF NOT EXISTS `idx_stats_slow_queries_deleted_at` ON `stats_slow_queries`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_stats_slow_time` ON `stats_slow_queries`(`timestamp`);
CREATE INDEX IF NOT EXISTS `idx_stats_user_activity_deleted_at` ON `stats_user_activity`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_stats_user_id` ON `stats_user_activity`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_stats_user_name` ON `stats_user_activity`(`username`);
CREATE INDEX IF NOT EXISTS `idx_stats_user_unique` ON `stats_user_activity`(`hour_timestamp`,`user_id`);
CREATE INDEX IF NOT EXISTS `idx_task_contact_contact_id` ON `task_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_task_contact_deleted_at` ON `task_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_task_contact_task_id` ON `task_contact`(`task_id`);
CREATE INDEX IF NOT EXISTS `idx_task_project_deleted_at` ON `task_project`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_task_project_project_id` ON `task_project`(`project_id`);
CREATE INDEX IF NOT EXISTS `idx_task_project_task_id` ON `task_project`(`task_id`);
CREATE INDEX IF NOT EXISTS `idx_task_site_deleted_at` ON `task_site`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_task_site_site_id` ON `task_site`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_task_site_task_id` ON `task_site`(`task_id`);
CREATE INDEX IF NOT EXISTS `idx_task_updates_created_by` ON `task_updates`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_task_updates_deleted_at` ON `task_updates`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_task_updates_status` ON `task_updates`(`status`);
CREATE INDEX IF NOT EXISTS idx_task_updates_task_created ON task_updates(task_id, created_time);
CREATE INDEX IF NOT EXISTS `idx_task_updates_task_id` ON `task_updates`(`task_id`);
CREATE INDEX IF NOT EXISTS `idx_tasks_actual_completion_date` ON `tasks`(`actual_completion_date`);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned_l1_status ON tasks(assigned_to_l1, status);
CREATE INDEX IF NOT EXISTS `idx_tasks_assigned_to_contact` ON `tasks`(`assigned_to_contact`);
CREATE INDEX IF NOT EXISTS `idx_tasks_assigned_to_l1` ON `tasks`(`assigned_to_l1`);
CREATE INDEX IF NOT EXISTS `idx_tasks_category` ON `tasks`(`category`);
CREATE INDEX IF NOT EXISTS `idx_tasks_created_by` ON `tasks`(`created_by`);
CREATE INDEX IF NOT EXISTS `idx_tasks_deleted_at` ON `tasks`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_tasks_end_date` ON `tasks`(`end_date`);
CREATE INDEX IF NOT EXISTS `idx_tasks_expected_completion_date` ON `tasks`(`expected_completion_date`);
CREATE INDEX IF NOT EXISTS `idx_tasks_priority` ON `tasks`(`priority`);
CREATE INDEX IF NOT EXISTS `idx_tasks_site_id` ON `tasks`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_tasks_site_status ON tasks(site_id, status);
CREATE INDEX IF NOT EXISTS `idx_tasks_start_date` ON `tasks`(`start_date`);
CREATE INDEX IF NOT EXISTS `idx_tasks_status` ON `tasks`(`status`);
CREATE INDEX IF NOT EXISTS idx_tasks_status_priority ON tasks(status, priority);
CREATE INDEX IF NOT EXISTS idx_tasks_title ON tasks(title);
CREATE INDEX IF NOT EXISTS `idx_tasks_type` ON `tasks`(`type`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_building_id` ON `telecom_room`(`building_id`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_contact_contact_id` ON `telecom_room_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_contact_deleted_at` ON `telecom_room_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_contact_telecom_room_id` ON `telecom_room_contact`(`telecom_room_id`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_deleted_at` ON `telecom_room`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_room_type` ON `telecom_room`(`room_type`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_site_id` ON `telecom_room`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_telecom_room_status` ON `telecom_room`(`status`);
CREATE INDEX IF NOT EXISTS `idx_tokens_deleted_at` ON `tokens`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_tokens_expires_at` ON `tokens`(`expires_at`);
CREATE INDEX IF NOT EXISTS `idx_tokens_token` ON `tokens`(`token`);
CREATE INDEX IF NOT EXISTS idx_tokens_user_expires ON tokens(user_id, expires_at);
CREATE INDEX IF NOT EXISTS `idx_tokens_user_id` ON `tokens`(`user_id`);
CREATE INDEX IF NOT EXISTS `idx_transport_contact_contact_id` ON `transport_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_transport_contact_deleted_at` ON `transport_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_transport_contact_transport_id` ON `transport_contact`(`transport_id`);
CREATE INDEX IF NOT EXISTS `idx_transport_vendor_deleted_at` ON `transport_vendor`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_transport_vendor_name` ON `transport_vendor`(`name`);
CREATE INDEX IF NOT EXISTS `idx_transport_vendor_transport_id` ON `transport_vendor`(`transport_id`);
CREATE INDEX IF NOT EXISTS `idx_transports_carrier_id` ON `transports`(`carrier_id`);
CREATE INDEX IF NOT EXISTS `idx_transports_deleted_at` ON `transports`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_transports_name` ON `transports`(`name`);
CREATE INDEX IF NOT EXISTS idx_transports_service_type ON transports(service_type);
CREATE INDEX IF NOT EXISTS `idx_transports_status` ON `transports`(`status`);
CREATE INDEX IF NOT EXISTS idx_transports_status_type ON transports(status, transport_type);
CREATE INDEX IF NOT EXISTS `idx_transports_transport_type` ON `transports`(`transport_type`);
CREATE INDEX IF NOT EXISTS idx_transports_vendor ON transports(vendor);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_contact_contact_id` ON `unplannedevent_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_contact_deleted_at` ON `unplannedevent_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_unplannedevent_contact_event_id ON unplannedevent_contact(unplanned_event_id);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_contact_unplanned_event_id` ON `unplannedevent_contact`(`unplanned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_location_deleted_at` ON `unplannedevent_location`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_location_location_id` ON `unplannedevent_location`(`location_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_location_unplanned_event_id` ON `unplannedevent_location`(`unplanned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_network_deleted_at` ON `unplannedevent_network`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_network_network_id` ON `unplannedevent_network`(`network_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_network_unplanned_event_id` ON `unplannedevent_network`(`unplanned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_service_deleted_at` ON `unplannedevent_service`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_service_service_id` ON `unplannedevent_service`(`service_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_service_unplanned_event_id` ON `unplannedevent_service`(`unplanned_event_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_updates_created_by_user` ON `unplannedevent_updates`(`createdbyuser`);
CREATE INDEX IF NOT EXISTS idx_unplannedevent_updates_createdbyuser ON unplannedevent_updates(createdbyuser);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_updates_deleted_at` ON `unplannedevent_updates`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_updates_status` ON `unplannedevent_updates`(`status`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_updates_time` ON `unplannedevent_updates`(`time`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_updates_unplanned_event_id` ON `unplannedevent_updates`(`unplannedid`);
CREATE INDEX IF NOT EXISTS idx_unplannedevent_updates_unplannedid ON unplannedevent_updates(unplannedid);
CREATE INDEX IF NOT EXISTS `idx_unplannedevent_updates_updated_by` ON `unplannedevent_updates`(`updatedby`);
CREATE INDEX IF NOT EXISTS idx_unplannedevent_updates_updatedby ON unplannedevent_updates(updatedby);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_assigned_to` ON `unplannedevents`(`assignedto`);
CREATE INDEX IF NOT EXISTS idx_unplannedevents_cir ON unplannedevents(cir);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_create_time` ON `unplannedevents`(`create_time`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_created_by_user` ON `unplannedevents`(`createdbyuser`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_deleted_at` ON `unplannedevents`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_event_type` ON `unplannedevents`(`eventype`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_ima_number` ON `unplannedevents`(`ima_number`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_initial_id` ON `unplannedevents`(`initial_id`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_legacy_id` ON `unplannedevents`(`legacyid`);
CREATE INDEX IF NOT EXISTS idx_unplannedevents_level ON unplannedevents(level);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_master_ticket` ON `unplannedevents`(`master_ticket`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_oia` ON `unplannedevents`(`oia`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_resolved_time` ON `unplannedevents`(`resolved_time`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_root_cause_found_time` ON `unplannedevents`(`root_cause_found_time`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_start_time` ON `unplannedevents`(`start_time`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_status` ON `unplannedevents`(`status`);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_tech_impact_id` ON `unplannedevents`(`techimpact_id`);
CREATE INDEX IF NOT EXISTS idx_unplannedevents_title ON unplannedevents(title);
CREATE INDEX IF NOT EXISTS `idx_unplannedevents_updated_by` ON `unplannedevents`(`updatedby`);
CREATE INDEX IF NOT EXISTS `idx_user_group` ON `users_group`(`user_id`,`group_id`);
CREATE INDEX IF NOT EXISTS `idx_user_smc` ON `users_smc`(`user_id`,`smc_id`);
CREATE INDEX IF NOT EXISTS `idx_users_api_key` ON `users`(`api_key`);
CREATE INDEX IF NOT EXISTS `idx_users_default_l1_id` ON `users`(`default_l1_id`);
CREATE INDEX IF NOT EXISTS `idx_users_default_site_id` ON `users`(`default_site_id`);
CREATE INDEX IF NOT EXISTS `idx_users_default_smc_id` ON `users`(`default_smc_id`);
CREATE INDEX IF NOT EXISTS `idx_users_deleted_at` ON `users`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_users_email` ON `users`(`email`);
CREATE INDEX IF NOT EXISTS `idx_users_group_deleted_at` ON `users_group`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_users_smc_deleted_at` ON `users_smc`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_users_username` ON `users`(`username`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_deleted_at` ON `usersreport`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_l1_id` ON `usersreport`(`l1_id`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_network_id` ON `usersreport`(`network_id`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_reporter_email` ON `usersreport`(`reporter_email`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_reporter_ip` ON `usersreport`(`reporter_ip`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_reporter_name` ON `usersreport`(`reporter_name`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_service_id` ON `usersreport`(`service_id`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_site_id` ON `usersreport`(`site_id`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_smc_id` ON `usersreport`(`smc_id`);
CREATE INDEX IF NOT EXISTS `idx_usersreport_status` ON `usersreport`(`status`);
CREATE INDEX IF NOT EXISTS `idx_vrf_deleted_at` ON `vrf`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_vrf_description ON vrf(description);
CREATE INDEX IF NOT EXISTS `idx_vrf_name` ON `vrf`(`name`);
CREATE INDEX IF NOT EXISTS idx_wifi_allow_byod ON wifi(allow_byod);
CREATE INDEX IF NOT EXISTS idx_wifi_ap_count ON wifi(ap_count);
CREATE INDEX IF NOT EXISTS `idx_wifi_contact_contact_id` ON `wifi_contact`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_wifi_contact_deleted_at` ON `wifi_contact`(`deleted_at`);
CREATE INDEX IF NOT EXISTS `idx_wifi_contact_id` ON `wifi`(`contact_id`);
CREATE INDEX IF NOT EXISTS `idx_wifi_contact_wi_fi_id` ON `wifi_contact`(`wi_fi_id`);
CREATE INDEX IF NOT EXISTS `idx_wifi_deleted_at` ON `wifi`(`deleted_at`);
CREATE INDEX IF NOT EXISTS idx_wifi_frequency_band ON wifi(frequency_band);
CREATE INDEX IF NOT EXISTS `idx_wifi_site_id` ON `wifi`(`site_id`);
CREATE INDEX IF NOT EXISTS idx_wifi_site_ssid ON wifi(site_id, ss_id);
CREATE INDEX IF NOT EXISTS `idx_wifi_ss_id` ON `wifi`(`ss_id`);
CREATE INDEX IF NOT EXISTS idx_wifi_standard ON wifi(wi_fi_standard);

CREATE VIEW IF NOT EXISTS sites_list_view AS
SELECT 
    s.*,
    COALESCE(l1.name, '-') AS l1_name,
    COALESCE(l1.french_name, COALESCE(l1.name, '-')) AS l1_french_name,
    COALESCE(smc.name, '-') AS smc_name,
    COALESCE(sat.name, '-') AS site_access_type_name,
    COALESCE(sdp_agg.cnt, 0) AS sdp_count,
    COALESCE(bldg_agg.cnt, 0) AS building_count,
    COALESCE(tr_agg.cnt, 0) AS telecom_room_count
FROM sites s
LEFT JOIN l1 ON l1.id = s.l1_id
LEFT JOIN smc ON smc.id = s.smc_id
LEFT JOIN site_access_type sat ON sat.id = s.site_access_type_id
LEFT JOIN (
    SELECT site_id, COUNT(1) AS cnt 
    FROM site_sdp 
    WHERE deleted_at IS NULL 
    GROUP BY site_id
) sdp_agg ON sdp_agg.site_id = s.id
LEFT JOIN (
    SELECT site_id, COUNT(1) AS cnt 
    FROM buildings 
    WHERE deleted_at IS NULL 
    GROUP BY site_id
) bldg_agg ON bldg_agg.site_id = s.id
LEFT JOIN (
    SELECT site_id, COUNT(1) AS cnt 
    FROM telecom_room 
    WHERE deleted_at IS NULL 
    GROUP BY site_id
) tr_agg ON tr_agg.site_id = s.id;
