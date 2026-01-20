CREATE FUNCTION [dbo].[GetUsersByName](@SearchName NVARCHAR(100))
RETURNS @Results TABLE (
    Id INT,
    Name NVARCHAR(100),
    Email NVARCHAR(255)
)
AS
BEGIN
    INSERT INTO @Results
    SELECT Id, Name, Email
    FROM dbo.Users
    WHERE Name LIKE '%' + @SearchName + '%';
    RETURN;
END
GO
