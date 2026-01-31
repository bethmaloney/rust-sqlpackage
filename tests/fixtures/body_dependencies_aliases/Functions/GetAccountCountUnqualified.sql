-- Function with unqualified table names (should trigger the alias bug)
CREATE FUNCTION [dbo].[GetAccountCountUnqualified]
(
    @Status NVARCHAR(20)
)
RETURNS INT
AS
BEGIN
    DECLARE @Count INT;

    SELECT @Count = COUNT(*)
    FROM Account A
    WHERE A.Status = @Status;

    RETURN @Count;
END;
GO
