CREATE FUNCTION [Sales].[GetOrderTotal](@OrderId INT)
RETURNS DECIMAL(18, 2)
AS
BEGIN
    DECLARE @Total DECIMAL(18, 2);
    SELECT @Total = SUM([Quantity] * [UnitPrice])
    FROM [Sales].[OrderItems]
    WHERE [OrderId] = @OrderId;
    RETURN ISNULL(@Total, 0);
END
GO
