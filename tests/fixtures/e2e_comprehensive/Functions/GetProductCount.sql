CREATE FUNCTION [dbo].[GetProductCount]()
RETURNS INT
AS
BEGIN
    DECLARE @Count INT;
    SELECT @Count = COUNT(*) FROM [dbo].[Products] WHERE [IsActive] = 1;
    RETURN @Count;
END
GO
