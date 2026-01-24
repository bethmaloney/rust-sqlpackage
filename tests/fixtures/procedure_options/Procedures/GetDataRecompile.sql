-- Procedure WITH RECOMPILE (recompiles on each execution)
CREATE PROCEDURE [dbo].[GetDataRecompile]
    @FilterValue INT
WITH RECOMPILE
AS
BEGIN
    SELECT * FROM [dbo].[AuditLog]
    WHERE [Id] > @FilterValue;
END;
GO
