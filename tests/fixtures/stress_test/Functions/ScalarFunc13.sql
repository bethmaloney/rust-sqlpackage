CREATE FUNCTION [dbo].[ScalarFunc13]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table14]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
