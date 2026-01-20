CREATE FUNCTION [dbo].[GetActiveUsers]()
RETURNS TABLE
AS
RETURN (
    SELECT Id, Name, Email
    FROM dbo.Users
);
GO
