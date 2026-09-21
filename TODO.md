To be able to migrate Overwatch.

1. Key delivery and Unlock
Overwatch must unlock Galvanize using the HTTP post 

2. Air-Gap files
Overwatch would have to uploads it's documents to the airgap bypassing Galvanize.  Overwatch API would need to check if the file exists before it 
can be downloaded by the user.

3. Schema management
Overwatch need to stop the auto-migration and creating schema, this will be done by Galvanize via schema.sql file.
 Schema Management:                                                      
      GORM's AutoMigrate must be completely disabled. All schemas 
      (tables, indexes, views like init.go:1230) are declared in Galvanize . 
      sql schema files with SELECT crsql_as_crr(...) on replicated tables.
** must create a schema.sql base on existing overwatch code +100 tables,
   make sure to identify the one that don't replicate 
   make sure to follow the rules on what's allow in the schema.sql base on 
   superfly/corrosion

4. IN Overwatch Database - update:
 Relax Secondary UNIQUE Indexes on Replicated Tables: In multi-master     
  CRDTs, asynchronous nodes cannot enforce distributed secondary uniqueness. 
  Convert secondary uniqueIndex tags on replicated tables to standard non-   
  unique indexes.
 Adopt Deterministic UUIDv5 for Named Entities: Use UUIDv5(namespace,     
  entity_name) when creating named entities (such as Sites and Projects) so  
  that concurrent creations of the same entity on different nodes share the  
  same ID and merge cleanly without creating duplicates.
  

