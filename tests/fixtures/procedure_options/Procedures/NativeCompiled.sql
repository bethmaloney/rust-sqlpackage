-- Procedure WITH NATIVE_COMPILATION and SCHEMABINDING (for memory-optimized tables)
-- Note: Requires memory-optimized filegroup in actual deployment
CREATE PROCEDURE [dbo].[NativeCompiled]
    @Id INT
WITH NATIVE_COMPILATION, SCHEMABINDING
AS
BEGIN ATOMIC WITH (TRANSACTION ISOLATION LEVEL = SNAPSHOT, LANGUAGE = N'English')
    SELECT [Id], [Action], [Timestamp]
    FROM [dbo].[AuditLog]
    WHERE [Id] = @Id;
END;
GO
