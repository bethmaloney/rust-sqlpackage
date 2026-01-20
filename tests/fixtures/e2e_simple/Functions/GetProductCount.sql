CREATE FUNCTION [dbo].[GetProductCount]()
RETURNS INT
AS
BEGIN
    DECLARE @Count INT;
    SELECT @Count = COUNT(*) FROM [dbo].[Products];
    RETURN @Count;
END
GO
