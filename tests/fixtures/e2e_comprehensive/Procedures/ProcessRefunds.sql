-- Procedure with multiple OUTPUT parameters
CREATE PROCEDURE [Sales].[ProcessRefunds]
    @OrderId INT,
    @RefundAmount DECIMAL(18, 2),
    @RefundId INT OUTPUT,
    @RefundDate DATETIME OUTPUT,
    @Success BIT OUTPUT
AS
BEGIN
    SET NOCOUNT ON;

    SET @RefundId = 1;
    SET @RefundDate = GETDATE();
    SET @Success = 1;

    SELECT @OrderId AS ProcessedOrderId, @RefundAmount AS Amount;
END
GO
