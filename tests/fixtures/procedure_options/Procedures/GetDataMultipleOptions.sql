-- Procedure with multiple options
CREATE PROCEDURE [dbo].[GetDataMultipleOptions]
    @Action NVARCHAR(100) = NULL
WITH RECOMPILE, EXECUTE AS OWNER
AS
BEGIN
    SET NOCOUNT ON;

    IF @Action IS NULL
        SELECT * FROM [dbo].[AuditLog];
    ELSE
        SELECT * FROM [dbo].[AuditLog] WHERE [Action] = @Action;
END;
GO
