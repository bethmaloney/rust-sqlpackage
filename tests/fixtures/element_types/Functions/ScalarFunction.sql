CREATE FUNCTION [dbo].[GetUserCount]()
RETURNS INT
AS
BEGIN
    DECLARE @Count INT;
    SELECT @Count = COUNT(*) FROM dbo.Users;
    RETURN @Count;
END
GO
