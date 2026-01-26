-- Procedure WITH SCHEMABINDING
CREATE PROCEDURE [dbo].[BoundProcedure]
    @Id INT
WITH SCHEMABINDING
AS
BEGIN
    SELECT [Id], [Action], [Timestamp]
    FROM [dbo].[AuditLog]
    WHERE [Id] = @Id;
END;
GO
