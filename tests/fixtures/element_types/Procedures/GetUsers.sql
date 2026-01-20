CREATE PROCEDURE [dbo].[GetUsers]
    @SearchTerm NVARCHAR(100) = NULL
AS
BEGIN
    SET NOCOUNT ON;

    SELECT Id, Name, Email
    FROM dbo.Users
    WHERE @SearchTerm IS NULL OR Name LIKE '%' + @SearchTerm + '%';
END
GO
