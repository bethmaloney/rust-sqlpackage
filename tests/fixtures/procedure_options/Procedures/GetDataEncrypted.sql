-- Procedure WITH ENCRYPTION (hides source code)
CREATE PROCEDURE [dbo].[GetDataEncrypted]
    @StartDate DATETIME,
    @EndDate DATETIME
WITH ENCRYPTION
AS
BEGIN
    SELECT [Id], [Action], [Timestamp]
    FROM [dbo].[AuditLog]
    WHERE [Timestamp] BETWEEN @StartDate AND @EndDate;
END;
GO
